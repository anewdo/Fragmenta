//! 分类与标签编辑弹窗：居中卡片 + 遮罩，打开 / 关闭均带淡入淡出。
//!
//! 使用 gpui-base 的无样式 Dialog 宿主（焦点陷阱 / Esc / 点击遮罩关闭）
//!
//! 关闭不是立即销毁：先把阶段切到 `Closing` 播放反向动画，计时结束后再
//! 释放输入实体并归还焦点。

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use fragmenta_core::db::Result as DbResult;
use fragmenta_core::model::Note;

use gpui_kit::assets::IconName;
use gpui_kit::base::Dialog;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::animation::{EffectTransition, ease_out_cubic};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::NotesPage;
use crate::ui::tokens::{self, TextRole, TextRoleExt};
use crate::widgets::tag_badge::tag_outline_chip;

/// 弹窗卡片内边距（p-5）
const MODAL_PADDING: Pixels = px(20.);
/// 弹窗卡片圆角（rounded-xl）
const MODAL_RADIUS: Pixels = px(12.);
/// 打开 / 关闭动画时长（duration-200）
const MODAL_ANIM: Duration = Duration::from_millis(200);

/// 弹窗阶段：关闭动画期间仍保持挂载，计时结束才真正移除。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModalPhase {
    Open,
    Closing,
}

/// 弹窗内标签草稿：添加 / 移除只改草稿，保存后才落库。
struct TaxonomyDraft {
    tags: Vec<String>,
}

/// 弹窗会话状态：输入实体、草稿、原始值与动画阶段。
pub(super) struct TaxonomyModal {
    note_id: i64,
    category_input: Entity<InputState>,
    tag_input: Entity<InputState>,
    draft: Entity<TaxonomyDraft>,
    tag_colors: HashMap<String, Option<Hsla>>,
    original_category: Option<String>,
    original_tags: Vec<String>,
    phase: ModalPhase,
    /// 动画序号：阶段切换时递增，让淡入淡出以新 id 重新播放。
    seq: usize,
    /// 焦点陷阱宿主焦点（Esc / Tab 在弹窗内循环）。
    focus: FocusHandle,
    /// 打开前的焦点，关闭动画结束后归还。
    previous_focus: Option<FocusHandle>,
}

impl NotesPage {
    /// 打开某条笔记的分类与标签编辑弹窗。
    pub(super) fn open_taxonomy_dialog(
        &mut self,
        note: Note,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial_category = note.category.clone().unwrap_or_default();
        let category_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("留空则不设分类");
            if !initial_category.is_empty() {
                state.set_value(initial_category, window, cx);
            }
            state
        });
        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("输入标签…"));
        let draft = cx.new(|_| TaxonomyDraft {
            tags: note.tags.clone(),
        });

        self.taxonomy_seq = self.taxonomy_seq.wrapping_add(1);
        let focus = cx.focus_handle();
        self.taxonomy_modal = Some(TaxonomyModal {
            note_id: note.id,
            category_input,
            tag_input,
            draft,
            tag_colors: self.tag_colors.clone(),
            original_category: note.category.clone(),
            original_tags: note.tags.clone(),
            phase: ModalPhase::Open,
            seq: self.taxonomy_seq,
            focus: focus.clone(),
            previous_focus: window.focused(cx),
        });
        focus.focus(window, cx);
        cx.notify();
    }

    /// 渲染弹窗（无弹窗时返回 None）。
    pub(super) fn render_taxonomy_modal(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let modal = self.taxonomy_modal.as_ref()?;
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let focus = modal.focus.clone();
        let closing = modal.phase == ModalPhase::Closing;
        let seq = modal.seq;
        let (from, to) = if closing { (1., 0.) } else { (0., 1.) };
        let (slide_from, slide_to) = if closing {
            (px(0.), px(6.))
        } else {
            (px(6.), px(0.))
        };
        let phase = if closing { "close" } else { "open" };
        let weak = cx.entity().downgrade();

        // 遮罩：黑 40%，随阶段淡入淡出
        let backdrop = EffectTransition::new(MODAL_ANIM)
            .ease(ease_out_cubic)
            .fade(from, to)
            .apply(
                div().absolute().size_full().bg(hsla(0., 0., 0., 0.4)),
                format!("taxonomy-backdrop-{phase}-{seq}"),
            )
            .into_any_element();

        let category_input = modal.category_input.clone();
        let tag_input = modal.tag_input.clone();
        let draft = modal.draft.clone();
        let tag_colors = modal.tag_colors.clone();
        let note_id = modal.note_id;
        let original_category = modal.original_category.clone();
        let original_tags = modal.original_tags.clone();

        let save: Rc<dyn Fn(&mut Window, &mut App) -> bool> = Rc::new({
            let weak = weak.clone();
            let category_input = category_input.clone();
            let tag_input = tag_input.clone();
            let draft = draft.clone();
            move |window, cx| {
                let category = {
                    let value = category_input.read(cx).value().trim().to_string();
                    (!value.is_empty()).then_some(value)
                };
                let mut tags = draft.read(cx).tags.clone();
                // 输入框里尚未点「添加」的标签随保存一并带上
                let pending = tag_input.read(cx).value().trim().to_string();
                if !pending.is_empty() && !tags.iter().any(|tag| tag == &pending) {
                    tags.push(pending);
                }
                if category == original_category && tags == original_tags {
                    return true;
                }
                let result = weak.update(cx, |page, cx| -> DbResult<()> {
                    if category != original_category {
                        page.notes.update(cx, |state, cx| {
                            state.set_category(note_id, category.as_deref(), cx)
                        })?;
                    }
                    if tags != original_tags {
                        page.notes
                            .update(cx, |state, cx| state.set_tags(note_id, tags.clone(), cx))?;
                    }
                    Ok(())
                });
                match result {
                    Ok(Ok(())) => {
                        window.push_notification(Notification::success("分类与标签已更新"), cx);
                        true
                    }
                    Ok(Err(err)) => {
                        window.push_notification(Notification::error(err.to_string()), cx);
                        false
                    }
                    Err(_) => true,
                }
            }
        });

        let close = {
            let weak = weak.clone();
            move |window: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |page, cx| page.begin_close_taxonomy(window, cx));
            }
        };

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
                    .child(
                        Icon::new(IconName::Palette)
                            .size_4()
                            .text_color(theme.primary),
                    )
                    .child(div().text_role(TextRole::SectionTitle).child("分类与标签")),
            )
            .child(
                div()
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
                        let close = close.clone();
                        move |_, window, cx| close(window, cx)
                    })
                    .child(Icon::new(IconName::X).size_4()),
            );

        let tags = draft.read(cx).tags.clone();
        let tag_row = if tags.is_empty() {
            div()
                .text_role(TextRole::Meta)
                .text_color(muted)
                .child("暂无标签")
                .into_any_element()
        } else {
            h_flex()
                .flex_wrap()
                .gap_1()
                .children(tags.iter().enumerate().map(|(ix, tag)| {
                    let tag = tag.clone();
                    let color = tag_colors.get(&tag).copied().flatten();
                    let draft_remove = draft.clone();
                    let removed = tag.clone();
                    div()
                        .id(("taxonomy-tag-chip", ix))
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.7))
                        .on_click(move |_, window, cx| {
                            let _ = draft_remove.update(cx, |draft, cx| {
                                draft.tags.retain(|existing| existing != &removed);
                                cx.notify();
                            });
                            window.refresh();
                        })
                        .child(tag_outline_chip(tag, color, &theme))
                }))
                .into_any_element()
        };

        let add_tag = {
            let tag_input = tag_input.clone();
            let draft = draft.clone();
            Button::new("taxonomy-add-tag")
                .outline()
                .label("添加")
                .on_click(move |_, window, cx| {
                    let value = tag_input.read(cx).value().trim().to_string();
                    if value.is_empty() {
                        return;
                    }
                    let _ = draft.update(cx, |draft, cx| {
                        if !draft.tags.iter().any(|tag| tag == &value) {
                            draft.tags.push(value);
                            cx.notify();
                        }
                    });
                    tag_input.update(cx, |state, cx| state.set_value("", window, cx));
                    window.refresh();
                })
        };

        let body = v_flex()
            .gap(tokens::spacing::MD)
            .child(
                v_flex()
                    .gap(tokens::spacing::XS)
                    .child(
                        div()
                            .text_role(TextRole::ControlLabel)
                            .text_color(muted)
                            .child("分类"),
                    )
                    .child(
                        div()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(Input::new(&category_input).w_full()),
                    ),
            )
            .child(
                v_flex()
                    .gap(tokens::spacing::XS)
                    .child(
                        div()
                            .text_role(TextRole::ControlLabel)
                            .text_color(muted)
                            .child("标签"),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap(tokens::spacing::SM)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .child(Input::new(&tag_input).w_full()),
                            )
                            .child(add_tag),
                    )
                    .child(tag_row),
            );

        let footer = h_flex()
            .justify_end()
            .gap(tokens::spacing::SM)
            .child(
                Button::new("taxonomy-cancel")
                    .outline()
                    .label("取消")
                    .on_click({
                        let close = close.clone();
                        move |_, window, cx| close(window, cx)
                    }),
            )
            .child(
                Button::new("taxonomy-save")
                    .primary()
                    .label("保存")
                    .on_click({
                        let save = save.clone();
                        let close = close.clone();
                        move |_, window, cx| {
                            if save(window, cx) {
                                close(window, cx);
                            }
                        }
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
            .child(header)
            .child(body)
            .child(footer);

        let card = EffectTransition::new(MODAL_ANIM)
            .ease(ease_out_cubic)
            .slide_y(slide_from, slide_to)
            .fade(from, to)
            .apply(card, format!("taxonomy-card-{phase}-{seq}"))
            .into_any_element();

        let close_for_ok = close.clone();
        let dialog = Dialog::new(cx)
            .focus_handle(modal.focus.clone())
            .dismiss_below_y(tokens::heights::TITLE_BAR)
            .backdrop(backdrop)
            .popup(card)
            .on_ok(move |_, window, cx| save(window, cx))
            .request_close(move |_, window, cx| close_for_ok(window, cx));

        Some(dialog.into_any_element())
    }

    /// 进入关闭阶段：播放反向动画，计时结束后移除弹窗并归还焦点。
    pub(super) fn begin_close_taxonomy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

        let weak = cx.entity().downgrade();
        cx.spawn_in(window, async move |_, cx| {
            cx.background_executor().timer(MODAL_ANIM).await;
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
