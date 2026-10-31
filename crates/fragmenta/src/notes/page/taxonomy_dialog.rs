//! 标签编辑弹窗
//!
//! 使用 gpui-base 的无样式 Dialog 宿主

use std::rc::Rc;

use fragmenta_core::model::Note;

use gpui_kit::assets::IconName;
use gpui_kit::base::Dialog;
use gpui_kit::base::actions::Confirm;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::NotesPage;
use crate::notes::components::TagChip;
use crate::taxonomy::TaxonomyStore;
use crate::ui::dialog_motion::DialogMotion;
use crate::ui::focus::{TextFieldExt as _, input_border};
use crate::ui::tokens::{self, TextRole, TextRoleExt};
use crate::utils::palette::format_hex_color;
use crate::widgets::color_picker::{
    ColorSwatchTrigger, color_popover_content, create_picker, fallback_picker_color,
};
use crate::widgets::toast;

/// 弹窗卡片内边距（p-5）
const MODAL_PADDING: Pixels = px(20.);
/// 弹窗卡片圆角（rounded-xl）
const MODAL_RADIUS: Pixels = px(12.);
/// 标签列表最大高度：超出后滚动，避免弹窗随标签数量无限增高。
const TAG_LIST_MAX_HEIGHT: Pixels = px(168.);

/// 弹窗阶段：关闭动画期间仍保持挂载，计时结束才真正移除。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModalPhase {
    Open,
    Closing,
}

/// 弹窗会话状态：输入实体、已选标签、取色器与动画阶段。
pub(super) struct TaxonomyModal {
    note_id: i64,
    tag_input: Entity<InputState>,
    picker: Entity<ColorPickerState>,
    /// 草稿选中标签：保存时才写回笔记，取消即丢弃。
    selected: Vec<String>,
    /// 新增标签将要使用的颜色；`None` 跟随主题（中性标签样式）。
    new_tag_color: Option<Hsla>,
    /// 取色 Popover 是否展开（受控）。
    color_open: bool,
    phase: ModalPhase,
    /// 动画序号：阶段切换时递增，让淡入淡出以新 id 重新播放。
    seq: usize,
    /// 焦点陷阱宿主焦点（Esc / Tab 在弹窗内循环）。
    focus: FocusHandle,
    /// 打开前的焦点，关闭动画结束后归还。
    previous_focus: Option<FocusHandle>,
    /// 输入框回车 → 添加标签。
    _tag_sub: Subscription,
    /// 取色器滑杆 / hex 变更 → 更新新增标签颜色。
    _picker_sub: Subscription,
}

impl NotesPage {
    /// 打开某条笔记的标签编辑弹窗。
    pub(super) fn open_taxonomy_dialog(
        &mut self,
        note: Note,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("输入标签…"));
        let picker = create_picker(window, cx, None);

        let weak = cx.entity().downgrade();
        let tag_sub = window.subscribe(&tag_input, cx, {
            let weak = weak.clone();
            move |input, event, window, app| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let value = input.read(app).value().to_string();
                    let _ = weak.update(app, |page, cx| page.add_taxonomy_tag(value, window, cx));
                }
            }
        });
        let picker_sub = cx.subscribe(&picker, move |this, _, event: &ColorPickerEvent, cx| {
            if let ColorPickerEvent::Change(Some(color)) = event {
                if let Some(modal) = this.taxonomy_modal.as_mut() {
                    modal.new_tag_color = Some(*color);
                }
                cx.notify();
            }
        });

        self.taxonomy_seq = self.taxonomy_seq.wrapping_add(1);
        let focus = cx.focus_handle();
        self.taxonomy_modal = Some(TaxonomyModal {
            note_id: note.id,
            tag_input,
            picker,
            selected: note.tags,
            new_tag_color: None,
            color_open: false,
            phase: ModalPhase::Open,
            seq: self.taxonomy_seq,
            focus: focus.clone(),
            previous_focus: window.focused(cx),
            _tag_sub: tag_sub,
            _picker_sub: picker_sub,
        });
        focus.focus(window, cx);
        cx.notify();
    }

    /// 点击标签：只切换草稿选中态，不写库也不刷新卡片（保存时统一提交）。
    fn toggle_taxonomy_tag(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(modal) = self.taxonomy_modal.as_mut() else {
            return;
        };
        if let Some(pos) = modal.selected.iter().position(|tag| tag == &name) {
            modal.selected.remove(pos);
        } else {
            modal.selected.push(name);
        }
        cx.notify();
    }

    /// 添加标签：标签实体按取色器颜色即时落库并加入草稿选中；标签已存在时
    /// 仅提示警告并保留输入内容，不自动勾选。笔记挂载仍然等到保存。
    fn add_taxonomy_tag(&mut self, raw: String, window: &mut Window, cx: &mut Context<Self>) {
        let name = raw.trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(modal) = self.taxonomy_modal.as_ref() else {
            return;
        };
        let tag_input = modal.tag_input.clone();
        let color = modal.new_tag_color.map(format_hex_color);

        let taxonomy = cx.global::<TaxonomyStore>().0.clone();
        let inserted = match taxonomy.update(cx, |state, cx| {
            state.ensure_tag(&name, color.as_deref(), cx)
        }) {
            Ok(inserted) => inserted,
            Err(err) => {
                toast::error(err.to_string(), window, cx);
                return;
            }
        };

        // 已存在的标签不再静默勾选：提示重复并保留输入内容，等用户改用别的名字
        if !inserted {
            toast::warn("标签已存在", window, cx);
            return;
        }
        if let Some(modal) = self.taxonomy_modal.as_mut() {
            modal.selected.push(name);
        }
        tag_input.update(cx, |state, cx| state.set_value("", window, cx));
        toast::success("标签保存成功", window, cx);
        cx.notify();
    }

    /// 保存语义：把草稿整体写回笔记；失败时保持弹窗打开并提示。
    fn commit_taxonomy_tags(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(modal) = self.taxonomy_modal.as_ref() else {
            return true;
        };
        let note_id = modal.note_id;
        let tags = modal.selected.clone();
        match self
            .notes
            .update(cx, |state, cx| state.set_tags(note_id, tags, cx))
        {
            Ok(()) => {
                toast::success("笔记标签更新成功", window, cx);
                true
            }
            Err(err) => {
                toast::error(err.to_string(), window, cx);
                false
            }
        }
    }

    /// 渲染弹窗（无弹窗时返回 None）。
    pub(super) fn render_taxonomy_modal(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let modal = self.taxonomy_modal.as_ref()?;
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let focus = modal.focus.clone();
        let closing = modal.phase == ModalPhase::Closing;
        let seq = modal.seq;
        let motion = DialogMotion::for_window(window, cx);
        let tag_border = input_border(
            &theme,
            modal.tag_input.read(cx).focus_handle(cx).is_focused(window),
        );
        let phase = if closing { "close" } else { "open" };
        let weak = cx.entity().downgrade();

        let tag_input = modal.tag_input.clone();
        let picker = modal.picker.clone();
        let color_open = modal.color_open;
        let new_tag_color = modal.new_tag_color;
        let selected_tags = &modal.selected;

        let close_cancel: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new({
            let weak = weak.clone();
            move |window, cx| {
                let _ = weak.update(cx, |page, cx| page.begin_close_taxonomy(false, window, cx));
            }
        });
        let close_save: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new({
            let weak = weak.clone();
            move |window, cx| {
                let _ = weak.update(cx, |page, cx| page.begin_close_taxonomy(true, window, cx));
            }
        });

        // 遮罩：与 kit Dialog 共用主题 overlay，随阶段淡入淡出。点击即关闭；
        // toast 浮层在更高图层自行遮挡命中，点击它不会到达这里。
        let backdrop = motion
            .backdrop(!closing)
            .apply(
                div()
                    .absolute()
                    .size_full()
                    .bg(theme.overlay)
                    .on_mouse_down(MouseButton::Left, {
                        let close = close_cancel.clone();
                        move |event, window, cx| {
                            if event.position.y < tokens::heights::TITLE_BAR {
                                return;
                            }
                            close(window, cx);
                            cx.stop_propagation();
                        }
                    }),
                format!("taxonomy-backdrop-{phase}-{seq}"),
            )
            .into_any_element();

        let close_button = div()
            .id("taxonomy-close")
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

        // 取色器：收起时是矩形色块，展开为无色 / 色板 / 自定义滑杆
        let swatch = ColorSwatchTrigger::new(("taxonomy-tag-color", seq as u64), new_tag_color)
            .selected(color_open);
        let picker_for_open = picker.clone();
        let weak_open = weak.clone();
        let color_picker = Popover::new(("taxonomy-tag-color-popover", seq as u64))
            .anchor(Anchor::TopRight)
            .open(color_open)
            .on_open_change(move |open, window, cx| {
                let picker = picker_for_open.clone();
                let _ = weak_open.update(cx, move |this, cx| {
                    let mut sync = None;
                    if let Some(modal) = this.taxonomy_modal.as_mut() {
                        modal.color_open = *open;
                        if *open {
                            sync = Some(modal.new_tag_color.unwrap_or_else(fallback_picker_color));
                        }
                    }
                    if let Some(color) = sync {
                        picker.update(cx, |state, cx| state.set_value(color, window, cx));
                    }
                    cx.notify();
                });
            })
            .trigger(swatch)
            .content({
                let weak_select = weak.clone();
                move |_, window, cx| {
                    let weak_select = weak_select.clone();
                    color_popover_content(
                        new_tag_color,
                        &picker,
                        "跟随主题",
                        move |color, _window, cx| {
                            let _ = weak_select.update(cx, |this, cx| {
                                if let Some(modal) = this.taxonomy_modal.as_mut() {
                                    modal.new_tag_color = color;
                                    modal.color_open = false;
                                }
                                cx.notify();
                            });
                        },
                        window,
                        cx,
                    )
                }
            });

        // 标题行：左「标签」、右关闭
        let header = h_flex()
            .items_center()
            .justify_between()
            .gap(tokens::spacing::SM)
            .pb(tokens::spacing::SM)
            .border_b_1()
            .border_color(theme.border.opacity(0.6))
            .child(
                h_flex()
                    .items_center()
                    .gap(tokens::spacing::SM)
                    .child(Icon::new(IconName::Tags).size_4().text_color(theme.primary))
                    .child(div().text_role(TextRole::SectionTitle).child("标签")),
            )
            .child(close_button);

        let add_tag = {
            let tag_input = tag_input.clone();
            Button::new("taxonomy-add-tag")
                .outline()
                .label("添加")
                .on_click({
                    let weak = weak.clone();
                    move |_, window, cx| {
                        let value = tag_input.read(cx).value().to_string();
                        let _ =
                            weak.update(cx, |this, cx| this.add_taxonomy_tag(value, window, cx));
                    }
                })
        };

        let tag_list = if self.all_tags.is_empty() {
            div()
                .text_role(TextRole::Meta)
                .text_color(muted)
                .child("暂无标签，输入名称后点击添加")
                .into_any_element()
        } else {
            div()
                .max_h(TAG_LIST_MAX_HEIGHT)
                .overflow_y_scrollbar()
                .child(h_flex().flex_wrap().gap(tokens::spacing::SM).children(
                    self.all_tags.iter().enumerate().map(|(ix, entry)| {
                        let selected = selected_tags.contains(&entry.name);
                        let color = self.tag_colors.get(&entry.name).copied().flatten();
                        let name = entry.name.clone();
                        let weak = weak.clone();
                        TagChip::new(("taxonomy-tag-chip", ix), entry.name.clone())
                            .color(color)
                            .selected(selected)
                            .on_click(move |_, _window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.toggle_taxonomy_tag(name.clone(), cx)
                                });
                            })
                    }),
                ))
                .into_any_element()
        };

        // 输入行：输入框 → 取色色块（新增标签将使用的颜色）→ 添加
        let body = v_flex()
            .gap(tokens::spacing::MD)
            .child(
                h_flex()
                    .items_center()
                    .gap(tokens::spacing::SM)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(Input::new(&tag_input).w_full().text_field_focus(tag_border)),
                    )
                    .child(color_picker)
                    .child(add_tag),
            )
            .child(tag_list);

        let footer = h_flex()
            .justify_end()
            .gap(tokens::spacing::SM)
            .child(
                Button::new("taxonomy-cancel")
                    .outline()
                    .label("取消")
                    .on_click({
                        let close = close_cancel.clone();
                        move |_, window, cx| close(window, cx)
                    }),
            )
            .child(
                Button::new("taxonomy-save")
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
            // 点击卡片空白处时把焦点收回弹窗，输入框随之失焦；
            // 输入框自身会截断冒泡，避免点进输入框时被抢走焦点。
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus.focus(window, cx);
            })
            // 回车由输入框（添加标签）与按钮自行处理，不触发 Dialog 确认关闭
            .on_action(|_: &Confirm, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
            .child(footer);

        let card = motion
            .card(!closing)
            .apply(card, format!("taxonomy-card-{phase}-{seq}"))
            .into_any_element();

        let close_for_request = close_cancel.clone();
        let dialog = Dialog::new(cx)
            .focus_handle(modal.focus.clone())
            .dismiss_below_y(tokens::heights::TITLE_BAR)
            // 遮罩关闭由自定义 backdrop 处理（kit 包装层仅负责截断其下图层）
            .close_on_backdrop_press(false)
            .backdrop(backdrop)
            .popup(card)
            .on_ok(|_, _, _| false)
            .request_close(move |_, window, cx| close_for_request(window, cx));

        Some(dialog.into_any_element())
    }

    /// 进入关闭阶段：`save` 时先把草稿标签写回笔记，写库失败则保持弹窗；
    /// 随后播放反向动画，计时结束后移除弹窗并归还焦点。取消直接丢弃草稿。
    pub(super) fn begin_close_taxonomy(
        &mut self,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if save && !self.commit_taxonomy_tags(window, cx) {
            return;
        }
        let Some(modal) = self.taxonomy_modal.as_mut() else {
            return;
        };
        if modal.phase == ModalPhase::Closing {
            return;
        }
        modal.phase = ModalPhase::Closing;
        modal.seq = modal.seq.wrapping_add(1);
        let seq = modal.seq;
        cx.notify();
        let duration = DialogMotion::for_window(window, cx).duration();

        let weak = cx.entity().downgrade();
        cx.spawn_in(window, async move |_, cx| {
            cx.background_executor().timer(duration).await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if this
                    .taxonomy_modal
                    .as_ref()
                    .is_some_and(|modal| modal.seq == seq && modal.phase == ModalPhase::Closing)
                {
                    let previous = this
                        .taxonomy_modal
                        .take()
                        .and_then(|modal| modal.previous_focus);
                    if let Some(handle) = previous {
                        window.focus(&handle, cx);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
