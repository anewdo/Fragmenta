//! 标签管理页：流式胶囊列表 + 新建 / 编辑弹窗（会话见 [`TagModal`]）。

use fragmenta_core::model::TaxonomyEntry;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, InteractiveElementExt as _, Sizable as _, WindowExt as _, h_flex,
    v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::tag_modal::{TagModal, TagModalEvent};
use super::{TaxonomyEvent, TaxonomyState, TaxonomyStore};
use crate::notes::{NotesEvent, NotesStore};
use crate::ui::layout::{LayoutClass, content_column};
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};
use crate::utils::palette::parse_hex_color;
use crate::widgets::toast;

/// 胶囊内删除叉号的命中区边长。
const CHIP_ACTION: Pixels = px(18.);
/// 胶囊内颜色点直径。
const CHIP_DOT: Pixels = px(8.);
/// 内容列顶部额外留白：把新建按钮与胶囊压到顶部居中 toast 浮层之下。
const CONTENT_TOP_INSET: Pixels = px(40.);

/// 标签管理页：常驻于主窗口 shell，订阅笔记广播刷新列表。
pub(crate) struct TaxonomyPage {
    state: Entity<TaxonomyState>,
    /// 条目列表快照（名称排序）。
    list: Result<Vec<TaxonomyEntry>, String>,
    /// 新建 / 编辑弹窗会话：独立 Entity，会话内变更仅重绘弹窗子树。
    modal: Option<Entity<TagModal>>,
    /// 弹窗动画序号：跨会话递增，动画元素 id 不复用。
    modal_seq: usize,
    /// 笔记写操作会改变标签集合（挂载或建实体），订阅其广播刷新。
    _notes_changed: Subscription,
    /// 标签写操作（本页删除、弹窗提交、笔记页建实体）广播后刷新列表。
    _taxonomy_changed: Subscription,
    /// 当前弹窗的 `Closed` 订阅：随下次打开替换；关闭后指向已释放实体，
    /// 观察自动失效。
    _modal_closed: Option<Subscription>,
}

impl TaxonomyPage {
    pub(crate) fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<TaxonomyStore>().0.clone();
        let notes = cx.global::<NotesStore>().0.clone();
        let notes_changed = cx.subscribe(&notes, |this, _, _: &NotesEvent, cx| {
            this.refresh(cx);
        });
        let taxonomy_changed = cx.subscribe(&state, |this, _, _: &TaxonomyEvent, cx| {
            this.refresh(cx);
        });
        let mut this = Self {
            state,
            list: Ok(Vec::new()),
            modal: None,
            modal_seq: 0,
            _notes_changed: notes_changed,
            _taxonomy_changed: taxonomy_changed,
            _modal_closed: None,
        };
        this.refresh(cx);
        this
    }

    /// 重新拉取标签列表。
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let store = self.state.read(cx);
        self.list = store.tags().map_err(|err| err.to_string());
        cx.notify();
    }

    /// 按条目 id 查当前列表中的条目。
    fn entry(&self, id: i64) -> Option<TaxonomyEntry> {
        self.list
            .as_ref()
            .ok()?
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    /// 打开弹窗：`target` 为 `None` 时新建；编辑时带入现名与颜色。
    fn open_modal(&mut self, target: Option<i64>, window: &mut Window, cx: &mut Context<Self>) {
        let entry = target.and_then(|id| self.entry(id));
        self.modal_seq = self.modal_seq.wrapping_add(1);
        let seq = self.modal_seq;
        let state = self.state.clone();
        let modal = cx.new(|cx| TagModal::new(entry.as_ref(), seq, state, window, cx));
        self._modal_closed = Some(cx.subscribe(&modal, |this, _, _: &TagModalEvent, cx| {
            this.modal = None;
            cx.notify();
        }));
        self.modal = Some(modal);
        cx.notify();
    }

    /// 叉号删除：无引用直接删除；有引用时弹出居中确认。
    fn request_delete(
        &mut self,
        entry: &TaxonomyEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if entry.note_count == 0 {
            self.delete_entry(entry, window, cx);
        } else {
            self.confirm_delete(entry, window, cx);
        }
    }

    /// 直接删除（无引用）并经通知反馈；列表经 `Changed` 订阅刷新。
    fn delete_entry(&mut self, entry: &TaxonomyEntry, window: &mut Window, cx: &mut Context<Self>) {
        match self
            .state
            .update(cx, |state, cx| state.delete(&entry.name, cx))
        {
            Ok(()) => toast::success("标签已删除", window, cx),
            Err(err) => toast::error(err.to_string(), window, cx),
        }
    }

    /// 删除确认：被引用时警示使用数量，Danger 主按钮居中呈现。
    fn confirm_delete(
        &mut self,
        entry: &TaxonomyEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = entry.name.clone();
        let description = format!(
            "「{name}」被 {} 篇笔记使用，删除后将从这些笔记摘除该标签。",
            entry.note_count
        );
        let weak = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let weak = weak.clone();
            crate::ui::dialog_motion::alert_dialog(alert.confirm())
                .title("删除标签")
                .description(description.clone())
                .centered()
                .ok_text("删除")
                .ok_variant(ButtonVariant::Danger)
                .cancel_text("取消")
                .on_ok({
                    let weak = weak.clone();
                    let name = name.clone();
                    move |_, window, cx| {
                        weak.update(cx, |this, cx| {
                            if let Err(err) =
                                this.state.update(cx, |state, cx| state.delete(&name, cx))
                            {
                                toast::error(err.to_string(), window, cx);
                                return false;
                            }
                            toast::success("标签已删除", window, cx);
                            true
                        })
                        .unwrap_or(true)
                    }
                })
        });
    }

    /// 错误占位：说明 + 重试。
    fn render_error(&self, err: &str, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (danger, muted) = (theme.danger, theme.muted_foreground);
        let weak = cx.entity().downgrade();
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(div().text_color(danger).child("加载列表失败"))
            .child(
                div()
                    .text_role(TextRole::Meta)
                    .text_color(muted)
                    .child(err.to_string()),
            )
            .child(
                Button::new("taxonomy-retry")
                    .outline()
                    .small()
                    .label("重试")
                    .on_click(move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| this.refresh(cx));
                    }),
            )
            .into_any_element()
    }

    /// 空态：条目数为 0 时的引导。
    fn render_empty(&self) -> AnyElement {
        Empty::new()
            .header(
                EmptyHeader::new()
                    .media(
                        EmptyMedia::new()
                            .with_variant(EmptyMediaVariant::Icon)
                            .child(Icon::new(IconName::Tags)),
                    )
                    .title(EmptyTitle::new().child("还没有标签"))
                    .description(EmptyDescription::new().child("点击左上角「新建标签」开始创建")),
            )
            .into_any_element()
    }

    /// 流式胶囊：颜色点 + 名称 + 关联数 + 删除叉号；双击进入编辑。
    fn render_chip(
        &self,
        entry: &TaxonomyEntry,
        theme: &gpui_kit::component::Theme,
        weak: &WeakEntity<TaxonomyPage>,
    ) -> AnyElement {
        let color = entry.color.as_deref().and_then(parse_hex_color);
        let accent = color.unwrap_or(theme.muted_foreground);
        let background = color
            .map(|color| color.opacity(0.12))
            .unwrap_or(theme.secondary);
        let border = color
            .map(|color| color.opacity(0.4))
            .unwrap_or(theme.border);
        let dot = color.unwrap_or(theme.muted_foreground.opacity(0.5));
        let (muted, danger) = (theme.muted_foreground, theme.danger);
        let id = entry.id;

        h_flex()
            .id(("taxonomy-chip", id as u64))
            .items_center()
            .gap(tokens::spacing::XS)
            .pl(tokens::spacing::MD)
            .pr(tokens::spacing::XS)
            .py(tokens::spacing::XS)
            .rounded_full()
            .bg(background)
            .border_1()
            .border_color(border)
            .text_role(TextRole::Meta)
            .text_color(theme.foreground)
            .cursor_pointer()
            .hover(move |style| style.border_color(accent))
            .on_double_click({
                let weak = weak.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.open_modal(Some(id), window, cx));
                }
            })
            .child(div().flex_shrink_0().size(CHIP_DOT).rounded_full().bg(dot))
            .child(div().flex_shrink_0().child(format!("#{}", entry.name)))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(muted)
                    .child(format!("{} 篇", entry.note_count)),
            )
            .child(
                div()
                    .id(("taxonomy-chip-remove", id as u64))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .size(CHIP_ACTION)
                    .rounded_full()
                    .cursor_pointer()
                    .text_color(muted)
                    .hover(move |style| style.bg(danger.opacity(0.15)).text_color(danger))
                    .on_click({
                        let weak = weak.clone();
                        let entry = entry.clone();
                        move |_, window, cx| {
                            let _ =
                                weak.update(cx, |this, cx| this.request_delete(&entry, window, cx));
                            cx.stop_propagation();
                        }
                    })
                    .child(Icon::new(IconName::X).size_3()),
            )
            .into_any_element()
    }

    /// 流式布局：胶囊按行自动换行。
    fn render_list(
        &self,
        list: &[TaxonomyEntry],
        theme: &gpui_kit::component::Theme,
        weak: &WeakEntity<TaxonomyPage>,
    ) -> AnyElement {
        h_flex()
            .w_full()
            .flex_wrap()
            .gap(tokens::spacing::SM)
            .children(
                list.iter()
                    .map(|entry| self.render_chip(entry, theme, weak)),
            )
            .into_any_element()
    }
}

impl Render for TaxonomyPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = LayoutClass::from_width(window.viewport_size().width);
        let theme = cx.theme().clone();
        let weak = cx.entity().downgrade();

        let header = h_flex().w_full().items_center().child(
            Button::new("taxonomy-create")
                .primary()
                .small()
                .icon(IconName::Plus)
                .label("新建标签")
                .on_click({
                    let weak = weak.clone();
                    move |_, window, cx| {
                        let _ = weak.update(cx, |this, cx| this.open_modal(None, window, cx));
                    }
                }),
        );

        let body = match &self.list {
            Err(err) => self.render_error(err, cx),
            Ok(list) if list.is_empty() => v_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .items_center()
                .justify_center()
                .child(self.render_empty())
                .into_any_element(),
            Ok(list) => div()
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_y_scrollbar()
                .child(self.render_list(list, &theme, &weak))
                .into_any_element(),
        };

        let content = content_column(tokens::widths::TAXONOMY_CONTENT)
            .flex_1()
            .min_h_0()
            .mt(CONTENT_TOP_INSET)
            .gap(tokens::spacing::LG)
            .child(header)
            .child(body);

        v_flex()
            .size_full()
            .items_center()
            .p(layout.outer_padding())
            .child(content)
            .when_some(self.modal.clone(), |this, modal| this.child(modal))
    }
}

#[cfg(test)]
mod tests {
    use super::TaxonomyPage;
    use crate::notes::{NotesState, NotesStore};
    use crate::taxonomy::{TaxonomyState, TaxonomyStore};
    use crate::utils::palette::{format_hex_color, parse_hex_color};
    use crate::widgets::toast::{ToastState, ToastStore};
    use fragmenta_core::db::Db;
    use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, WindowOptions, hsla};
    use std::sync::Arc;

    /// 装配标签管理页（真实内存 Db + 依赖的 global 登记）。
    fn open_page(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<TaxonomyPage>, Arc<Db>) {
        cx.update(gpui_kit::init);
        let db = Arc::new(Db::open_in_memory().unwrap());
        let notes = cx.new(|_| NotesState::new(db.clone()));
        let taxonomy = cx.new(|_| TaxonomyState::new(db.clone()));
        let toast = cx.new(|_| ToastState::new());
        let (handle, page) = cx.update(|cx| {
            cx.set_global(NotesStore(notes));
            cx.set_global(TaxonomyStore(taxonomy));
            cx.set_global(ToastStore(toast));
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| TaxonomyPage::new(window, cx))
            })
            .unwrap()
        });
        (handle, page, db)
    }

    /// 打开弹窗并驱动一次提交：设名称、选色后 commit。
    /// 返回提交是否通过（供断言）。
    fn commit_via_modal(
        handle: AnyWindowHandle,
        page: &Entity<TaxonomyPage>,
        cx: &mut TestAppContext,
        target: Option<i64>,
        name: &str,
        color: Option<gpui_kit::Hsla>,
    ) -> bool {
        let mut committed = false;
        cx.update_window(handle, |_, window, cx| {
            page.update(cx, |page, cx| page.open_modal(target, window, cx));
            let modal = page.update(cx, |page, _| page.modal.clone().unwrap());
            modal.update(cx, |modal, cx| {
                let input = modal.name_input.clone();
                input.update(cx, |state, cx| state.set_value(name, window, cx));
                if let Some(color) = color {
                    modal.set_color(Some(color), window, cx);
                }
                committed = modal.commit(window, cx);
            });
        })
        .unwrap();
        committed
    }

    /// 新建：名称与颜色一并落库，成功后列表经订阅刷新。
    #[gpui_kit::test]
    fn modal_create_persists_name_and_color(cx: &mut TestAppContext) {
        let (handle, page, db) = open_page(cx);
        let color = hsla(0.5, 0.6, 0.7, 1.);

        assert!(commit_via_modal(
            handle,
            &page,
            cx,
            None,
            "工作",
            Some(color)
        ));
        // Changed 事件在更新周期结束投递，列表刷新后再断言。
        page.update(cx, |page, _| {
            assert_eq!(page.list.as_ref().unwrap().len(), 1);
        });
        assert_eq!(
            db.tag_colors().unwrap().get("工作").cloned().flatten(),
            Some(format_hex_color(color))
        );
    }

    /// 编辑：双击入口带入现名，提交后改名并改色。
    #[gpui_kit::test]
    fn modal_edit_renames_and_recolors(cx: &mut TestAppContext) {
        let (handle, page, db) = open_page(cx);

        assert!(commit_via_modal(handle, &page, cx, None, "工作", None));
        let id = page.update(cx, |page, _| page.list.as_ref().unwrap()[0].id);

        assert!(commit_via_modal(
            handle,
            &page,
            cx,
            Some(id),
            "职业",
            parse_hex_color("#2196F3")
        ));
        page.update(cx, |page, _| {
            assert_eq!(page.list.as_ref().unwrap()[0].name, "职业");
        });
        assert_eq!(
            db.tag_colors().unwrap().get("职业").cloned().flatten(),
            Some(String::from("#2196F3"))
        );
    }

    /// 无引用标签：叉号直接删除，不弹确认。
    #[gpui_kit::test]
    fn chip_delete_removes_tag_without_references(cx: &mut TestAppContext) {
        let (handle, page, db) = open_page(cx);

        assert!(commit_via_modal(handle, &page, cx, None, "临时", None));
        cx.update_window(handle, |_, window, cx| {
            let entry = page.update(cx, |page, _| page.list.as_ref().unwrap()[0].clone());
            assert_eq!(entry.note_count, 0);
            page.update(cx, |page, cx| page.request_delete(&entry, window, cx));
        })
        .unwrap();
        page.update(cx, |page, _| {
            assert!(page.list.as_ref().unwrap().is_empty());
        });
        assert!(db.tags_with_usage().unwrap().is_empty());
    }

    /// 空名称不提交，弹窗保持打开。
    #[gpui_kit::test]
    fn blank_name_keeps_modal_open(cx: &mut TestAppContext) {
        let (handle, page, db) = open_page(cx);

        assert!(!commit_via_modal(handle, &page, cx, None, "  ", None));
        page.update(cx, |page, _| assert!(page.modal.is_some()));
        assert!(db.tags_with_usage().unwrap().is_empty());
    }
}
