//! 系统字体选择弹窗：从启动时读取的本机字体族中搜索，选中后追加进
//! 字体下拉并立即应用到发起方（编辑器 / 小窗排版）。
//!
//! 与标签弹窗同一会话组件模式：搜索词等会话内变更只重绘弹窗子树；
//! 关闭动画结束后广播 [`FontPickerEvent::Closed`]，由宿主移除弹窗。

use std::rc::Rc;

use gpui_kit::SharedString;
use gpui_kit::assets::IconName;
use gpui_kit::base::Dialog;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::SettingsPage;
use crate::settings::fonts;
use crate::settings::{FontScope, SettingsStore};
use crate::ui::dialog_motion::DialogMotion;
use crate::ui::focus::{TextFieldExt as _, input_border};
use crate::ui::modal_session::{ModalSession, ModalSessionHost, begin_close};
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};

/// 弹窗卡片内边距（p-5）。
const MODAL_PADDING: Pixels = px(20.);
/// 弹窗卡片圆角（rounded-xl）。
const MODAL_RADIUS: Pixels = px(12.);
/// 字体列表可视高度：固定高度让弹窗尺寸稳定，列表内部滚动；
/// 配合标题 / 搜索框 / 说明后整体高度不超过主窗口最小高度。
const LIST_HEIGHT: Pixels = px(240.);
/// 列表行高：uniform_list 要求等高。
const ROW_HEIGHT: Pixels = px(32.);

/// 关闭动画结束（焦点归还完成后）广播，宿主据此移除弹窗。
pub(super) enum FontPickerEvent {
    Closed,
}

impl EventEmitter<FontPickerEvent> for FontPicker {}

/// 系统字体选择会话：本机字体族快照 + 搜索框 + 虚拟列表。
pub(super) struct FontPicker {
    /// 选中字体应用的排版位。
    scope: FontScope,
    query: Entity<InputState>,
    _query_sub: Subscription,
    /// 本机字体族快照：构造时读取一次，弹窗生命周期内不变。
    installed: Rc<Vec<String>>,
    /// 与 `installed` 同序的小写名，搜索只在预计算的小写名上做包含判断。
    installed_lower: Rc<Vec<String>>,
    /// 内置候选中本机已安装的族名（行内“内置”标记）。
    builtin: Rc<Vec<String>>,
    /// 已加入两份字体下拉的自定义字体族名（行内“已添加”标记）。
    added: Rc<Vec<String>>,
    /// 搜索命中的 `installed` 下标；只在搜索词变化时重建。
    matched: Rc<Vec<usize>>,
    /// 列表滚动位置：搜索词变化时回到顶部。
    scroll: UniformListScrollHandle,
    /// 关闭动画、动画序号与焦点归还。
    session: ModalSession,
}

impl ModalSessionHost for FontPicker {
    fn modal_session(&mut self) -> Option<&mut ModalSession> {
        Some(&mut self.session)
    }
}

impl FontPicker {
    pub(super) fn new(
        scope: FontScope,
        seq: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // 先取打开前的焦点，再聚焦搜索框（否则取到的是搜索框自身）
        let previous_focus = window.focused(cx);
        let (installed, added) = {
            let state = cx.global::<SettingsStore>().0.read(cx);
            (
                state.installed_font_names_shared(),
                state.custom_font_families().to_vec(),
            )
        };
        let installed_lower = installed
            .iter()
            .map(|family| family.to_lowercase())
            .collect();
        let builtin = fonts::installed_builtins(&installed);
        let matched = (0..installed.len()).collect();
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("搜索本机字体…"));
        let query_sub = cx.subscribe(&query, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refresh_matched(cx);
                this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                cx.notify();
            }
        });
        query.update(cx, |state, cx| state.focus(window, cx));
        Self {
            scope,
            query,
            _query_sub: query_sub,
            installed,
            installed_lower: Rc::new(installed_lower),
            builtin: Rc::new(builtin),
            added: Rc::new(added),
            matched: Rc::new(matched),
            scroll: UniformListScrollHandle::new(),
            session: ModalSession::new(seq, cx.focus_handle(), previous_focus),
        }
    }

    /// 按当前搜索词重建命中下标：小写名已预计算，这里只做包含判断。
    fn refresh_matched(&mut self, cx: &Context<Self>) {
        let query = self.query.read(cx).value().to_lowercase();
        let matched = if query.is_empty() {
            (0..self.installed_lower.len()).collect()
        } else {
            self.installed_lower
                .iter()
                .enumerate()
                .filter(|(_, lower)| lower.contains(&query))
                .map(|(ix, _)| ix)
                .collect()
        };
        self.matched = Rc::new(matched);
    }

    /// 选中字体：追加到自定义候选并应用到发起方排版位，随后播放关闭动画。
    fn pick(&mut self, family: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.is_closing() {
            return;
        }
        let store = cx.global::<SettingsStore>().0.clone();
        let scope = self.scope;
        store.update(cx, |state, cx| state.apply_system_font(scope, family, cx));
        self.begin_close(window, cx);
    }

    /// 进入关闭阶段：播放反向动画，计时结束后归还焦点并广播 `Closed`。
    fn begin_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        begin_close(&mut self.session, window, cx, |_, _, cx| {
            cx.emit(FontPickerEvent::Closed)
        });
    }
}

/// 宿主侧的弹窗挂载：维护唯一一个 `FontPicker` 实体及其关闭订阅。
#[derive(Default)]
pub(super) struct FontPickerHost {
    picker: Option<Entity<FontPicker>>,
    /// 动画序号：每次打开递增，保证跨会话的动画元素 id 不复用。
    seq: usize,
    _subscription: Option<Subscription>,
}

impl FontPickerHost {
    /// 打开系统字体选择弹窗；已打开时不再重复创建。
    pub(super) fn open(
        &mut self,
        scope: FontScope,
        window: &mut Window,
        cx: &mut Context<SettingsPage>,
    ) {
        if self.picker.is_some() {
            return;
        }
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;
        let picker = cx.new(|cx| FontPicker::new(scope, seq, window, cx));
        self._subscription = Some(cx.subscribe(&picker, |page, _, _: &FontPickerEvent, cx| {
            page.typography.font_picker.close();
            cx.notify();
        }));
        self.picker = Some(picker);
        cx.notify();
    }

    /// 渲染用弹窗句柄；`None` 时不挂载。
    pub(super) fn entity(&self) -> Option<Entity<FontPicker>> {
        self.picker.clone()
    }

    /// 弹窗当前是否打开。
    pub(super) fn is_open(&self) -> bool {
        self.picker.is_some()
    }

    fn close(&mut self) {
        self.picker = None;
        self._subscription = None;
    }
}

/// 列表行内右侧标记：内置候选 / 已添加。
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowMark {
    None,
    Builtin,
    Added,
}

impl Render for FontPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let border = theme.border;
        let danger = theme.danger;
        let accent = theme.accent;
        let popover = theme.tokens.popover;
        let closing = self.session.is_closing();
        let seq = self.session.seq();
        let motion = DialogMotion::for_window(window, cx);
        let weak = cx.entity().downgrade();

        let installed = self.installed.clone();
        let builtin = self.builtin.clone();
        let added = self.added.clone();
        let matched = self.matched.clone();
        let empty = matched.is_empty();

        let close_cancel = {
            let weak = weak.clone();
            move |window: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |this, cx| this.begin_close(window, cx));
            }
        };

        let backdrop = motion
            .backdrop(!closing)
            .apply(
                div().absolute().size_full().bg(theme.overlay),
                format!("font-picker-backdrop-{seq}"),
            )
            .into_any_element();

        let query_focused = self.query.read(cx).focus_handle(cx).is_focused(window);
        let query_border = input_border(&theme, query_focused);

        let close_button = div()
            .id("font-picker-close")
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .w(tokens::heights::CONTROL)
            .h(tokens::heights::CONTROL)
            .rounded(tokens::radius::SM)
            .cursor_pointer()
            .text_color(muted)
            .hover(move |style| style.bg(danger.opacity(0.12)).text_color(danger))
            .active(move |style| style.bg(danger.opacity(0.2)).text_color(danger))
            .on_click({
                let close = close_cancel.clone();
                move |_, window, cx| close(window, cx)
            })
            .child(Icon::new(IconName::X).size_4());

        let header = h_flex()
            .items_center()
            .justify_between()
            .gap(tokens::spacing::SM)
            .child(
                div()
                    .text_role(TextRole::SectionTitle)
                    .child("添加系统字体"),
            )
            .child(close_button);

        let row_weak = weak.clone();
        let list = uniform_list("font-picker-list", matched.len(), move |range, _, _| {
            range
                .filter_map(|row| {
                    let family = SharedString::from(installed.get(*matched.get(row)?)?.clone());
                    let mark = if fonts::contains_family(builtin.as_slice(), &family) {
                        RowMark::Builtin
                    } else if fonts::contains_family(added.as_slice(), &family) {
                        RowMark::Added
                    } else {
                        RowMark::None
                    };
                    Some(font_row(row, family, mark, accent, muted, row_weak.clone()))
                })
                .collect::<Vec<_>>()
        })
        .h_full()
        .track_scroll(&self.scroll);

        let body = if empty {
            div()
                .flex()
                .items_center()
                .justify_center()
                .w_full()
                .h(tokens::heights::CONTROL_LARGE)
                .text_role(TextRole::Meta)
                .text_color(muted)
                .child("未找到匹配的本机字体")
                .into_any_element()
        } else {
            div()
                .w_full()
                .h(LIST_HEIGHT)
                .overflow_hidden()
                .child(list)
                .into_any_element()
        };

        let hint = div()
            .text_role(TextRole::Meta)
            .text_color(muted)
            .child("选择后立即应用，并加入两份字体下拉；标记“内置”的字体已在候选中");

        let footer = h_flex().justify_end().child(
            Button::new("font-picker-done")
                .outline()
                .label("关闭")
                .on_click({
                    let close = close_cancel.clone();
                    move |_, window, cx| close(window, cx)
                }),
        );

        let card = v_flex()
            .relative()
            .occlude()
            .w(tokens::widths::DIALOG)
            .gap(tokens::spacing::LG)
            .p(MODAL_PADDING)
            .rounded(MODAL_RADIUS)
            .bg(popover)
            .border_1()
            .border_color(border)
            .shadow_lg()
            .child(header)
            .child(
                Input::new(&self.query)
                    .w_full()
                    .text_field_focus(query_border),
            )
            .child(body)
            .child(hint)
            .child(footer);

        let card = motion
            .card(!closing)
            .apply(card, format!("font-picker-card-{seq}"))
            .into_any_element();

        let focus = self.session.focus().clone();
        let weak_request = weak.clone();
        let dialog = Dialog::new(cx)
            .focus_handle(focus)
            .dismiss_below_y(tokens::heights::TITLE_BAR)
            .backdrop(backdrop)
            .popup(card)
            .request_close(move |_, window, cx| {
                let _ = weak_request.update(cx, |this, cx| this.begin_close(window, cx));
            });

        dialog.into_any_element()
    }
}

/// 单行字体：名称以自身字体族渲染，右侧标注内置 / 已添加；内置项不可点。
fn font_row(
    ix: usize,
    family: SharedString,
    mark: RowMark,
    accent: Hsla,
    muted: Hsla,
    weak: WeakEntity<FontPicker>,
) -> impl IntoElement {
    let clickable = mark != RowMark::Builtin;
    h_flex()
        .id(("font-picker-row", ix))
        .h(ROW_HEIGHT)
        .w_full()
        .px(tokens::spacing::SM)
        .gap(tokens::spacing::SM)
        .items_center()
        .justify_between()
        .rounded(tokens::radius::SM)
        .when(clickable, |el| {
            el.cursor_pointer()
                .hover(move |style| style.bg(accent.opacity(0.12)))
        })
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_role(TextRole::ControlLabel)
                .font_family(family.clone())
                .child(family.clone()),
        )
        .when(mark != RowMark::None, |el| {
            el.child(
                div()
                    .flex_shrink_0()
                    .text_role(TextRole::Meta)
                    .text_color(muted)
                    .child(match mark {
                        RowMark::Builtin => "内置",
                        _ => "已添加",
                    }),
            )
        })
        .when(clickable, |el| {
            el.on_click(move |_, window, cx| {
                let _ = weak.update(cx, |this, cx| this.pick(family.to_string(), window, cx));
            })
        })
}
