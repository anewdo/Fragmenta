//! 分类 / 标签管理页视图：两页复用同一组件（`TaxonomyKind` 区分装配）。
//!

use fragmenta_core::model::TaxonomyEntry;

use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{TaxonomyKind, TaxonomyState, TaxonomyStore};
use crate::notes::{NotesEvent, NotesStore};
use crate::ui::layout::{LayoutClass, content_column};
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};
use crate::utils::palette::parse_hex_color;

/// 行色点直径。
const COLOR_DOT: Pixels = px(10.);

/// 行内重命名会话：同一时刻至多一行；输入框为本次会话的全新实体
///（不同行 / 多次进入互不残留），回车提交、失焦放弃；会话 drop 自动退订。
struct RenameSession {
    /// 正在重命名的条目 id。
    id: i64,
    input: Entity<InputState>,
    _sub: Subscription,
}

/// 分类 / 标签管理页：常驻于主窗口 shell，订阅笔记广播刷新列表。
pub(crate) struct TaxonomyPage {
    kind: TaxonomyKind,
    state: Entity<TaxonomyState>,
    /// 新建输入行：回车即建（常驻于页面）。
    create_input: Entity<InputState>,
    /// 行内重命名会话。
    rename: Option<RenameSession>,
    /// 条目列表快照（名称排序）。
    list: Result<Vec<TaxonomyEntry>, String>,
    /// 笔记写操作会改变分类 / 标签集合（挂载或建实体），订阅其广播刷新。
    _notes_changed: Subscription,
    /// 新建输入行的回车订阅。
    _create_sub: Subscription,
}

impl TaxonomyPage {
    pub(crate) fn new(kind: TaxonomyKind, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<TaxonomyStore>().0.clone();
        let notes = cx.global::<NotesStore>().0.clone();
        let notes_changed = cx.subscribe(&notes, |this, _, _: &NotesEvent, cx| {
            this.refresh(cx);
        });
        let create_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(kind.create_placeholder()));
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
            kind,
            state,
            create_input,
            rename: None,
            list: Ok(Vec::new()),
            _notes_changed: notes_changed,
            _create_sub: create_sub,
        };
        this.refresh(cx);
        this
    }

    /// 重新拉取本种类条目列表。
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let store = self.state.read(cx);
        self.list = self.kind.entries(store).map_err(|err| err.to_string());
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
        let kind = self.kind;
        let result = self
            .state
            .update(cx, |state, cx| state.create(kind, name, cx));
        match result {
            Ok(()) => {
                self.create_input
                    .update(cx, |state, cx| state.set_value("", window, cx));
                self.refresh(cx);
            }
            Err(err) => window.push_notification(Notification::error(err.to_string()), cx),
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
            let mut state = InputState::new(window, cx).placeholder(self.kind.label());
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
            let kind = self.kind;
            if let Err(err) = self
                .state
                .update(cx, |state, cx| state.rename(kind, &from, to, cx))
            {
                window.push_notification(Notification::error(err.to_string()), cx);
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

    /// 删除确认：Danger 主按钮；被引用时警示「N 篇笔记正在使用」。
    /// 确认后经 state 层删除（分类归属置空 / 标签挂载摘除由数据层完成），
    /// 失败以通知呈现。
    fn confirm_delete(
        &mut self,
        entry: &TaxonomyEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = self.kind;
        let name = entry.name.clone();
        let description = match entry.note_count {
            0 => format!("「{name}」删除后无法恢复，确定要删除吗？"),
            count => match kind {
                TaxonomyKind::Category => {
                    format!("{count} 篇笔记正在使用「{name}」，删除后这些笔记的归属将被置空。")
                }
                TaxonomyKind::Tag => {
                    format!("{count} 篇笔记正在使用「{name}」，删除后将从这些笔记摘除该标签。")
                }
            },
        };
        let weak = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let weak = weak.clone();
            alert
                .confirm()
                .title(format!("删除{}", kind.label()))
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
                                .update(cx, |state, cx| state.delete(kind, &name, cx))
                            {
                                window.push_notification(Notification::error(err.to_string()), cx);
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
                            .child(Icon::new(self.kind.icon())),
                    )
                    .title(EmptyTitle::new().child(self.kind.empty_title()))
                    .description(EmptyDescription::new().child(self.kind.empty_description())),
            )
            .into_any_element()
    }

    /// 条目列表：名称排序的行列表。
    fn render_list(
        &self,
        list: &[TaxonomyEntry],
        weak: &WeakEntity<TaxonomyPage>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        v_flex()
            .flex_1()
            .min_h_0()
            .gap(tokens::spacing::SM)
            .children(list.iter().map(|entry| self.render_row(entry, theme, weak)))
            .into_any_element()
    }

    /// 单行：色点 + 名称（截断 / 重命名时为输入框）+ 引用数 + 改名与删除。
    fn render_row(
        &self,
        entry: &TaxonomyEntry,
        theme: &gpui_kit::component::Theme,
        weak: &WeakEntity<TaxonomyPage>,
    ) -> AnyElement {
        let dot_color = entry.color.as_deref().and_then(parse_hex_color);
        let dot = match dot_color {
            Some(color) => div().size(COLOR_DOT).rounded_full().bg(color),
            None => div()
                .size(COLOR_DOT)
                .rounded_full()
                .border_1()
                .border_color(theme.border),
        };
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
            .child(dot);
        if let Some(session) = self
            .rename
            .as_ref()
            .filter(|session| session.id == entry.id)
        {
            // 重命名中：名称位替换为会话输入框，操作按钮隐藏
            row = row
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&session.input).w_full()),
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
        let create_row = content_column(tokens::widths::TAXONOMY_CONTENT)
            .flex_shrink_0()
            .child(Input::new(&self.create_input).w_full());
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
                .child(self.render_list(list, &weak, cx))
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
