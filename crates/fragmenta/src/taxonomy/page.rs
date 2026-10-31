//! 标签管理页视图。
//!

use fragmenta_core::model::TaxonomyEntry;

use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{TaxonomyEvent, TaxonomyState, TaxonomyStore};
use crate::notes::{NotesEvent, NotesStore};
use crate::ui::dialog_motion;
use crate::ui::focus::{TextFieldExt as _, input_border};
use crate::ui::layout::{LayoutClass, content_column};
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};
use crate::utils::palette::{format_hex_color, parse_hex_color};
use crate::widgets::color_picker::{ColorSwatchTrigger, color_popover_content, create_picker};
use crate::widgets::toast;

/// 行内重命名会话：同一时刻至多一行；输入框为本次会话的全新实体
///（不同行 / 多次进入互不残留），回车提交、失焦放弃；会话 drop 自动退订。
struct RenameSession {
    /// 正在重命名的条目 id。
    id: i64,
    input: Entity<InputState>,
    _sub: Subscription,
}

/// 选色弹层会话：弹层打开时创建（自定义取色器初值 = 当前颜色，未选色以
/// 色板蓝为起点），关闭或色板选定后释放；滑杆拖动 / hex 回车即时落库。
struct ColorSession {
    /// 正在选色的条目 id。
    id: i64,
    picker: Entity<ColorPickerState>,
    _sub: Subscription,
}

/// 标签管理页：常驻于主窗口 shell，订阅笔记广播刷新列表。
pub(crate) struct TaxonomyPage {
    state: Entity<TaxonomyState>,
    /// 新建输入行：回车即建（常驻于页面）。
    create_input: Entity<InputState>,
    /// 行内重命名会话。
    rename: Option<RenameSession>,
    /// 选色弹层会话（控制 Popover 的受控 open 状态）。
    color_popover: Option<ColorSession>,
    /// 条目列表快照（名称排序）。
    list: Result<Vec<TaxonomyEntry>, String>,
    /// 笔记写操作会改变标签集合（挂载或建实体），订阅其广播刷新。
    _notes_changed: Subscription,
    /// 标签编辑弹窗等外部入口建实体时同步刷新（管理页自身操作也会触发，重拉无害）。
    _taxonomy_changed: Subscription,
    /// 新建输入行的回车订阅。
    _create_sub: Subscription,
}

impl TaxonomyPage {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<TaxonomyStore>().0.clone();
        let notes = cx.global::<NotesStore>().0.clone();
        let notes_changed = cx.subscribe(&notes, |this, _, _: &NotesEvent, cx| {
            this.refresh(cx);
        });
        let taxonomy_changed = cx.subscribe(&state, |this, _, _: &TaxonomyEvent, cx| {
            this.refresh(cx);
        });
        let create_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("新建标签，回车保存")
        });
        // 回车新建：空白仅清空、重名以通知报错且保留输入、成功即清空刷新。
        let weak = cx.entity().downgrade();
        let create_sub = window.subscribe(&create_input, cx, move |input, event, window, app| {
            if !matches!(event, InputEvent::PressEnter { .. }) {
                return;
            }
            let name = input.read(app).value().to_string();
            let _ = weak.update(app, |this, cx| this.create(name, window, cx));
        });
        let mut this = Self {
            state,
            create_input,
            rename: None,
            color_popover: None,
            list: Ok(Vec::new()),
            _notes_changed: notes_changed,
            _taxonomy_changed: taxonomy_changed,
            _create_sub: create_sub,
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

    /// 新建输入行回车提交：trim 后空白仅清空；重名以通知报错并保留输入
    ///（可改后再试）；成功即刷新列表并清空输入。
    fn create(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let name = name.trim();
        if name.is_empty() {
            self.create_input
                .update(cx, |state, cx| state.set_value("", window, cx));
            return;
        }
        let result = self.state.update(cx, |state, cx| state.create(name, cx));
        match result {
            Ok(()) => {
                self.create_input
                    .update(cx, |state, cx| state.set_value("", window, cx));
                self.refresh(cx);
            }
            Err(err) => toast::error(err.to_string(), window, cx),
        }
    }

    /// 进入行内重命名：为该行创建全新 InputState（初值 = 现名）并自动聚焦；
    /// 覆盖旧会话（旧订阅随 drop 退订）。
    fn start_rename(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self
            .list
            .as_ref()
            .ok()
            .and_then(|entries| entries.iter().find(|entry| entry.id == id))
            .cloned()
        else {
            return;
        };
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("标签");
            state.set_value(entry.name, window, cx);
            state
        });
        let weak = cx.entity().downgrade();
        let sub = window.subscribe(&input, cx, move |input, event, window, app| match event {
            InputEvent::PressEnter { .. } => {
                let to = input.read(app).value().to_string();
                let _ = weak.update(app, |this, cx| this.commit_rename(id, &to, window, cx));
            }
            InputEvent::Blur => {
                let _ = weak.update(app, |this, cx| this.end_rename(id, cx));
            }
            _ => {}
        });
        input.update(cx, |state, cx| state.focus(window, cx));
        self.rename = Some(RenameSession {
            id,
            input,
            _sub: sub,
        });
        cx.notify();
    }

    /// 回车提交重命名：空白或未变更视为放弃；错误以通知呈现；
    /// 无论成败都结束会话（沿用既有对话框「报错即关」的行为）。
    fn commit_rename(&mut self, id: i64, to: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.rename.take_if(|session| session.id == id).is_none() {
            return;
        }
        let from = self
            .list
            .as_ref()
            .ok()
            .and_then(|entries| entries.iter().find(|entry| entry.id == id))
            .map(|entry| entry.name.clone());
        let Some(from) = from else {
            self.refresh(cx);
            return;
        };
        let to = to.trim();
        if !to.is_empty() && to != from {
            if let Err(err) = self
                .state
                .update(cx, |state, cx| state.rename(&from, to, cx))
            {
                toast::error(err.to_string(), window, cx);
            }
        }
        self.refresh(cx);
    }

    /// 失焦放弃重命名：仅结束会话，不落库。
    fn end_rename(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.rename.take_if(|session| session.id == id).is_some() {
            self.refresh(cx);
        }
    }

    /// 按条目 id 查当前列表中的名称。
    fn entry_name(&self, id: i64) -> Option<String> {
        self.list
            .as_ref()
            .ok()
            .and_then(|entries| entries.iter().find(|entry| entry.id == id))
            .map(|entry| entry.name.clone())
    }

    /// 打开选色弹层：为该行创建自定义取色器会话（hex 输入 + HSL 滑杆由
    /// kit `ColorPickerState` 提供，颜色变更经订阅即时落库）。
    fn open_color_popover(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .list
            .as_ref()
            .ok()
            .and_then(|entries| entries.iter().find(|entry| entry.id == id))
            .and_then(|entry| entry.color.as_deref())
            .and_then(parse_hex_color);
        let picker = create_picker(window, cx, current);
        let sub = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &ColorPickerEvent, window, cx| {
                if let ColorPickerEvent::Change(Some(color)) = event {
                    this.commit_custom_color(id, *color, window, cx);
                }
            },
        );
        self.color_popover = Some(ColorSession {
            id,
            picker,
            _sub: sub,
        });
        cx.notify();
    }

    /// 关闭选色弹层并释放会话（受控 open 的程序性关闭不再回调本方法）。
    fn close_color_popover(&mut self, id: i64, cx: &mut Context<Self>) {
        if self
            .color_popover
            .take_if(|session| session.id == id)
            .is_some()
        {
            cx.notify();
        }
    }

    /// 颜色落库：按 id 取名 → state 层写库（成功广播 Changed 同步笔记侧）。
    /// 返回是否成功（失败以通知呈现，弹层保持打开）。
    fn apply_color(
        &mut self,
        id: i64,
        color: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(name) = self.entry_name(id) else {
            return false;
        };
        match self
            .state
            .update(cx, |state, cx| state.set_color(&name, color, cx))
        {
            Ok(()) => {
                self.refresh(cx);
                true
            }
            Err(err) => {
                toast::error(err.to_string(), window, cx);
                false
            }
        }
    }

    /// 色板 / 无色选定：落库成功后关闭弹层。
    fn commit_palette_color(
        &mut self,
        id: i64,
        color: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.apply_color(id, color, window, cx) {
            self.close_color_popover(id, cx);
        }
    }

    /// 自定义取色（滑杆拖动 / hex 回车）：即时落库，弹层保持打开。
    fn commit_custom_color(
        &mut self,
        id: i64,
        color: Hsla,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let hex = format_hex_color(color);
        self.apply_color(id, Some(&hex), window, cx);
    }

    /// 删除确认：Danger 主按钮；被引用时警示「N 篇笔记正在使用」。
    /// 确认后经 state 层删除（挂载摘除由数据层完成），失败以通知呈现。
    fn confirm_delete(
        &mut self,
        entry: &TaxonomyEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = entry.name.clone();
        let description = match entry.note_count {
            0 => format!("「{name}」删除后无法恢复，确定要删除吗？"),
            count => {
                format!("{count} 篇笔记正在使用「{name}」，删除后将从这些笔记摘除该标签。")
            }
        };
        let weak = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let weak = weak.clone();
            dialog_motion::alert_dialog(alert.confirm())
                .title("删除标签")
                .description(description.clone())
                .ok_text("删除")
                .ok_variant(ButtonVariant::Danger)
                .cancel_text("取消")
                .on_ok({
                    let weak = weak.clone();
                    let name = name.clone();
                    move |_, window, cx| {
                        weak.update(cx, |this, cx| {
                            if let Err(err) = this
                                .state
                                .update(cx, |state, cx| state.delete(&name, cx))
                            {
                                toast::error(err.to_string(), window, cx);
                                return false;
                            }
                            this.refresh(cx);
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
            .items_center()
            .justify_center()
            .gap_2()
            .child(div().text_color(danger).child("加载列表失败"))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(muted)
                    .child(err.to_string()),
            )
            .child(
                Button::new("taxonomy-retry")
                    .outline()
                    .small()
                    .label("重试")
                    .on_click({
                        let weak = weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| this.refresh(cx));
                        }
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
                    .description(EmptyDescription::new()
                        .child("在上方输入行输入标签名，回车即可新建")),
            )
            .into_any_element()
    }

    /// 条目列表：名称排序的行列表。
    fn render_list(
        &self,
        list: &[TaxonomyEntry],
        window: &mut Window,
        weak: &WeakEntity<TaxonomyPage>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        v_flex()
            .flex_1()
            .min_h_0()
            .gap(tokens::spacing::SM)
            .children(
                list.iter()
                    .map(|entry| self.render_row(entry, theme, window, weak, cx)),
            )
            .into_any_element()
    }

    /// 单行：色块（点击弹选色 Popover）+ 名称（截断 / 重命名时为输入框）
    /// + 引用数 + 改名与删除。
    fn render_row(
        &self,
        entry: &TaxonomyEntry,
        theme: &gpui_kit::component::Theme,
        window: &mut Window,
        weak: &WeakEntity<TaxonomyPage>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let swatch_color = entry.color.as_deref().and_then(parse_hex_color);
        let mut color_popover = Popover::new(("taxonomy-color", entry.id as u64))
            .trigger(ColorSwatchTrigger::new(
                ("taxonomy-color-swatch", entry.id as u64),
                swatch_color,
            ))
            .open(
                self.color_popover
                    .as_ref()
                    .is_some_and(|session| session.id == entry.id),
            )
            .on_open_change({
                let weak = weak.clone();
                let id = entry.id;
                move |open, window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if *open {
                            this.open_color_popover(id, window, cx);
                        } else {
                            this.close_color_popover(id, cx);
                        }
                    });
                }
            });
        if let Some(session) = self
            .color_popover
            .as_ref()
            .filter(|session| session.id == entry.id)
        {
            let picker = session.picker.clone();
            let current = entry.color.as_deref().and_then(parse_hex_color);
            let weak_content = weak.clone();
            let id = entry.id;
            color_popover = color_popover.content(move |_, window, cx| {
                let weak_select = weak_content.clone();
                color_popover_content(
                    current,
                    &picker,
                    "无色",
                    move |color, window, cx| {
                        let hex = color.map(format_hex_color);
                        let _ = weak_select.update(cx, |this, cx| {
                            this.commit_palette_color(id, hex.as_deref(), window, cx)
                        });
                    },
                    window,
                    cx,
                )
            });
        }
        let count = div()
            .flex_shrink_0()
            .text_role(TextRole::Meta)
            .text_color(theme.muted_foreground)
            .child(format!("{} 篇", entry.note_count));
        let mut row = h_flex()
            .w_full()
            .items_center()
            .gap(tokens::spacing::MD)
            .px(tokens::spacing::LG)
            .py(tokens::spacing::MD)
            .rounded(tokens::radius::SM)
            .bg(theme.secondary)
            .border_1()
            .border_color(theme.border)
            .child(color_popover);
        if let Some(session) = self
            .rename
            .as_ref()
            .filter(|session| session.id == entry.id)
        {
            // 重命名中：名称位替换为会话输入框，操作按钮隐藏
            let rename_border = input_border(
                theme,
                session.input.read(cx).focus_handle(cx).is_focused(window),
            );
            row = row
                .child(
                    div().flex_1().min_w_0().child(
                        Input::new(&session.input)
                            .w_full()
                            .text_field_focus(rename_border),
                    ),
                )
                .child(count);
        } else {
            row = row
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_role(TextRole::ControlLabel)
                        .child(entry.name.clone()),
                )
                .child(count)
                .child(
                    Button::new(("taxonomy-rename", entry.id as u64))
                        .ghost()
                        .compact()
                        .xsmall()
                        .icon(IconName::PenLine)
                        .on_click({
                            let weak = weak.clone();
                            let id = entry.id;
                            move |_, window, cx| {
                                let _ =
                                    weak.update(cx, |this, cx| this.start_rename(id, window, cx));
                            }
                        }),
                )
                .child(
                    Button::new(("taxonomy-delete", entry.id as u64))
                        .ghost()
                        .compact()
                        .xsmall()
                        .icon(IconName::Trash)
                        .on_click({
                            let weak = weak.clone();
                            let entry = entry.clone();
                            move |_, window, cx| {
                                let _ = weak
                                    .update(cx, |this, cx| this.confirm_delete(&entry, window, cx));
                            }
                        }),
                );
        }
        row.into_any_element()
    }
}

impl Render for TaxonomyPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = LayoutClass::from_width(window.viewport_size().width);
        let weak = cx.entity().downgrade();
        // 钉顶新建行（不随列表滚动）+ 可滚动主体，骨架同笔记页筛选栏模式
        let create_border = input_border(
            cx.theme(),
            self.create_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
        );
        let create_row = content_column(tokens::widths::TAXONOMY_CONTENT)
            .flex_shrink_0()
            .child(
                Input::new(&self.create_input)
                    .w_full()
                    .text_field_focus(create_border),
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
            Ok(list) => content_column(tokens::widths::TAXONOMY_CONTENT)
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(self.render_list(list, window, &weak, cx))
                .into_any_element(),
        };
        v_flex()
            .size_full()
            .items_center()
            .gap(tokens::spacing::LG)
            .p(layout.outer_padding())
            .child(create_row)
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::TaxonomyPage;
    use crate::notes::{NotesState, NotesStore};
    use crate::taxonomy::{TaxonomyState, TaxonomyStore};
    use fragmenta_core::db::Db;
    use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, WindowOptions, hsla};
    use std::sync::Arc;

    /// 装配标签管理页（真实内存 Db + 依赖的 global 登记）。
    fn open_page(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<TaxonomyPage>, Arc<Db>) {
        cx.update(gpui_kit::init);
        let db = Arc::new(Db::open_in_memory().unwrap());
        let notes = cx.new(|_| NotesState::new(db.clone()));
        let taxonomy = cx.new(|_| TaxonomyState::new(db.clone()));
        let (handle, page) = cx.update(|cx| {
            cx.set_global(NotesStore(notes));
            cx.set_global(TaxonomyStore(taxonomy));
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| TaxonomyPage::new(window, cx))
            })
            .unwrap()
        });
        (handle, page, db)
    }

    /// 色板选定（含无色）：即时落库并结束弹层会话；自定义取色（滑杆 /
    /// hex 的 Change 事件）同样即时落库。
    #[gpui_kit::test]
    fn color_popover_commits_palette_none_and_custom_colors(cx: &mut TestAppContext) {
        let (handle, page, db) = open_page(cx);

        let id = cx
            .update_window(handle, |_, window, cx| {
                page.update(cx, |page, cx| {
                    page.create("工作".into(), window, cx);
                    let id = page.list.as_ref().unwrap()[0].id;
                    // 色板选定：落库 + 弹层关闭
                    page.open_color_popover(id, window, cx);
                    page.commit_palette_color(id, Some("#2196F3"), window, cx);
                    assert!(page.color_popover.is_none());
                    id
                })
            })
            .unwrap();
        assert_eq!(
            db.tag_colors().unwrap().get("工作").cloned().flatten(),
            Some(String::from("#2196F3"))
        );

        // 无色：恢复未选色
        cx.update_window(handle, |_, window, cx| {
            page.update(cx, |page, cx| {
                page.open_color_popover(id, window, cx);
                page.commit_palette_color(id, None, window, cx);
            });
        })
        .unwrap();
        assert_eq!(db.tag_colors().unwrap().get("工作"), Some(&None));

        // 自定义取色：滑杆变更（Change 事件）经订阅即时落库，弹层保持打开
        cx.update_window(handle, |_, window, cx| {
            page.update(cx, |page, cx| {
                page.open_color_popover(id, window, cx);
                let picker = page.color_popover.as_ref().unwrap().picker.clone();
                picker.update(cx, |state, cx| {
                    state.update_color(hsla(0.5, 0.6, 0.7, 1.), window, cx)
                });
                assert!(page.color_popover.is_some(), "自定义取色不关闭弹层");
            });
        })
        .unwrap();
        let committed = db
            .tag_colors()
            .unwrap()
            .get("工作")
            .cloned()
            .flatten()
            .expect("滑杆变更已落库");
        // 落库格式为不透明 #RRGGBB
        assert_eq!(
            committed,
            crate::utils::palette::format_hex_color(hsla(0.5, 0.6, 0.7, 1.))
        );
    }
}
