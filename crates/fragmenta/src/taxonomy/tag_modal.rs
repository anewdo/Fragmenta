//! 标签弹窗：新建 / 编辑共用的会话组件。
//!
//! 以独立 Entity 承载：取色拖动等会话内变更只通知自身、仅重绘弹窗子树，
//! 宿主页面不参与。落库经 `TaxonomyState` 广播 `Changed`，列表刷新由宿主的
//! 订阅完成（事件在更新周期结束投递）。关闭动画结束后广播
//! [`TagModalEvent::Closed`]，由宿主移除弹窗。

use std::rc::Rc;

use fragmenta_core::model::TaxonomyEntry;

use gpui_kit::assets::IconName;
use gpui_kit::base::Dialog;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::TaxonomyState;
use crate::ui::dialog_motion::DialogMotion;
use crate::ui::focus::{TextFieldExt as _, input_border};
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};
use crate::utils::palette::{format_hex_color, parse_hex_color};
use crate::widgets::color_picker::{color_picker_content, create_picker, fallback_picker_color};
use crate::widgets::toast;

/// 弹窗卡片内边距（p-5）。
const MODAL_PADDING: Pixels = px(20.);
/// 弹窗卡片圆角（rounded-xl）。
const MODAL_RADIUS: Pixels = px(12.);
/// 弹窗内容宽度：卡片宽度扣除左右内边距，内联取色器铺满该宽度。
const MODAL_CONTENT: Pixels = px(340.);

/// 弹窗阶段：关闭动画期间仍保持挂载，计时结束才真正移除。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModalPhase {
    Open,
    Closing,
}

/// 关闭动画结束（焦点归还完成后）广播，宿主据此移除弹窗。
pub(crate) enum TagModalEvent {
    Closed,
}

impl EventEmitter<TagModalEvent> for TagModal {}

/// 标签弹窗会话：新建（`target == None`）与编辑共用。
pub(crate) struct TagModal {
    /// 标签状态：提交时经其落库（成功广播 `Changed` 供宿主刷新列表）。
    state: Entity<TaxonomyState>,
    /// 编辑目标条目 id；`None` 为新建。
    target: Option<i64>,
    /// 编辑前的名称：重命名提交时作为 `from`，成功后更新为现值。
    original_name: Option<String>,
    /// 名称输入；`pub(crate)` 供宿主页面的模块内测试驱动。
    pub(crate) name_input: Entity<InputState>,
    picker: Entity<ColorPickerState>,
    /// 当前选色；`None` 为未选色（胶囊跟随主题中性样式）。
    color: Option<Hsla>,
    phase: ModalPhase,
    /// 动画序号：阶段切换时递增，让淡入淡出以新 id 重新播放。
    seq: usize,
    /// 焦点陷阱宿主焦点（Esc / Enter 在弹窗内响应）。
    focus: FocusHandle,
    /// 打开前的焦点，关闭动画结束后归还。
    previous_focus: Option<FocusHandle>,
    /// 取色器滑杆 / hex 变更 → 更新草稿颜色。
    _picker_sub: Subscription,
}

impl TagModal {
    /// 打开弹窗：`entry` 为 `None` 时新建；编辑时带入现名与颜色。`seq` 是
    /// 宿主派发的动画序号，保证跨会话的动画元素 id 不复用。
    pub(crate) fn new(
        entry: Option<&TaxonomyEntry>,
        seq: usize,
        state: Entity<TaxonomyState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (name, color) = match entry {
            Some(entry) => (
                entry.name.clone(),
                entry.color.as_deref().and_then(parse_hex_color),
            ),
            None => (String::new(), None),
        };
        let name_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("标签名称");
            if !name.is_empty() {
                input.set_value(name, window, cx);
            }
            input
        });
        let picker = create_picker(window, cx, color);
        let picker_sub = cx.subscribe(&picker, |this, _, event: &ColorPickerEvent, cx| {
            if let ColorPickerEvent::Change(Some(color)) = event {
                this.color = Some(*color);
                cx.notify();
            }
        });
        let this = Self {
            state,
            target: entry.map(|entry| entry.id),
            original_name: entry.map(|entry| entry.name.clone()),
            name_input: name_input.clone(),
            picker,
            color,
            phase: ModalPhase::Open,
            seq,
            focus: cx.focus_handle(),
            previous_focus: window.focused(cx),
            _picker_sub: picker_sub,
        };
        name_input.update(cx, |state, cx| state.focus(window, cx));
        this
    }

    /// 色板 / 无色选定：更新草稿颜色，并把取色器同步到同一颜色
    ///（未选色时以色板蓝作为滑杆起点）。
    pub(crate) fn set_color(
        &mut self,
        color: Option<Hsla>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.color = color;
        let value = color.unwrap_or_else(fallback_picker_color);
        self.picker
            .update(cx, |state, cx| state.set_value(value, window, cx));
        cx.notify();
    }

    /// 提交弹窗：新建时按名称与颜色建实体（重名保留弹窗），编辑时先改名再
    /// 写颜色。返回是否可以关闭弹窗。
    pub(crate) fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let name = self.name_input.read(cx).value().trim().to_string();
        let color = self.color.map(format_hex_color);
        let target = self.target;
        let original = self.original_name.clone();

        if name.is_empty() {
            toast::warn("标签名称不能为空", window, cx);
            return false;
        }

        match target {
            None => match self.state.update(cx, |state, cx| {
                state.ensure_tag(&name, color.as_deref(), cx)
            }) {
                Ok(true) => {
                    toast::success("标签已创建", window, cx);
                    true
                }
                Ok(false) => {
                    toast::warn("标签已存在", window, cx);
                    false
                }
                Err(err) => {
                    toast::error(err.to_string(), window, cx);
                    false
                }
            },
            Some(_) => {
                let Some(original) = original else {
                    return false;
                };
                if name != original {
                    if let Err(err) = self
                        .state
                        .update(cx, |state, cx| state.rename(&original, &name, cx))
                    {
                        toast::error(err.to_string(), window, cx);
                        return false;
                    }
                    self.original_name = Some(name.clone());
                }
                if let Err(err) = self
                    .state
                    .update(cx, |state, cx| state.set_color(&name, color.as_deref(), cx))
                {
                    toast::error(err.to_string(), window, cx);
                    return false;
                }
                toast::success("标签已更新", window, cx);
                true
            }
        }
    }

    /// 进入关闭阶段：`save` 时先提交，失败保持弹窗；随后播放反向动画，
    /// 计时结束后归还焦点并广播 `Closed` 交由宿主移除。
    fn begin_close(&mut self, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.phase == ModalPhase::Closing {
            return;
        }
        if save && !self.commit(window, cx) {
            return;
        }
        self.phase = ModalPhase::Closing;
        self.seq = self.seq.wrapping_add(1);
        cx.notify();
        let duration = DialogMotion::for_window(window, cx).duration();

        let weak = cx.entity().downgrade();
        cx.spawn_in(window, async move |_, cx| {
            cx.background_executor().timer(duration).await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if this.phase == ModalPhase::Closing {
                    if let Some(handle) = this.previous_focus.take() {
                        window.focus(&handle, cx);
                    }
                    cx.emit(TagModalEvent::Closed);
                }
            });
        })
        .detach();
    }
}

impl Render for TagModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let closing = self.phase == ModalPhase::Closing;
        let seq = self.seq;
        let focus = self.focus.clone();
        let motion = DialogMotion::for_window(window, cx);
        let weak = cx.entity().downgrade();

        let title = if self.target.is_some() {
            "编辑标签"
        } else {
            "新建标签"
        };
        let name_input = self.name_input.clone();
        let picker = self.picker.clone();
        let color = self.color;

        let close_cancel: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new({
            let weak = weak.clone();
            move |window, cx| {
                let _ = weak.update(cx, |this, cx| this.begin_close(false, window, cx));
            }
        });
        let close_save: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new({
            let weak = weak.clone();
            move |window, cx| {
                let _ = weak.update(cx, |this, cx| this.begin_close(true, window, cx));
            }
        });

        let backdrop = motion
            .backdrop(!closing)
            .apply(
                div().absolute().size_full().bg(theme.overlay),
                format!("taxonomy-modal-backdrop-{seq}"),
            )
            .into_any_element();

        let name_border = input_border(
            &theme,
            name_input.read(cx).focus_handle(cx).is_focused(window),
        );

        let color_content = color_picker_content(
            color,
            &picker,
            "无色",
            {
                let weak = weak.clone();
                move |color, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.set_color(color, window, cx));
                }
            },
            MODAL_CONTENT,
            window,
            cx,
        );

        let close_button = div()
            .id("taxonomy-modal-close")
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
            .child(div().text_role(TextRole::SectionTitle).child(title))
            .child(close_button);

        let footer = h_flex()
            .justify_end()
            .gap(tokens::spacing::SM)
            .child(
                Button::new("taxonomy-modal-cancel")
                    .outline()
                    .label("取消")
                    .on_click({
                        let close = close_cancel.clone();
                        move |_, window, cx| close(window, cx)
                    }),
            )
            .child(
                Button::new("taxonomy-modal-save")
                    .primary()
                    .label("保存")
                    .on_click({
                        let close = close_save.clone();
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
            .bg(theme.tokens.popover)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(header)
            .child(
                Input::new(&name_input)
                    .w_full()
                    .text_field_focus(name_border),
            )
            .child(
                v_flex()
                    .gap(tokens::spacing::SM)
                    .child(
                        div()
                            .text_role(TextRole::Meta)
                            .text_color(muted)
                            .child("标签颜色"),
                    )
                    .child(color_content),
            )
            .child(footer);

        let card = motion
            .card(!closing)
            .apply(card, format!("taxonomy-modal-card-{seq}"))
            .into_any_element();

        let weak_request = weak.clone();
        let dialog = Dialog::new(cx)
            .focus_handle(focus)
            .dismiss_below_y(tokens::heights::TITLE_BAR)
            .backdrop(backdrop)
            .popup(card)
            .request_close(move |save, window, cx| {
                let _ = weak_request.update(cx, |this, cx| this.begin_close(save, window, cx));
            });

        dialog.into_any_element()
    }
}
