//! 分类与标签编辑弹窗：标准 ContentDialog（分类输入 + 标签草稿 + 确定 / 取消）。
//!
//! 弹窗内只修改本地草稿，保存时才一次性落库并以 Toast 反馈结果；标签
//! 颜色继续取自页快照中的 DB 持久颜色（ADR-0008），未选色为中性样式。

use std::rc::Rc;

use fragmenta_core::db::Result as DbResult;
use fragmenta_core::model::Note;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::NotesPage;
use crate::ui::tokens::{self, TextRole, TextRoleExt};
use crate::widgets::tag_badge::tag_outline_chip;

/// 弹窗内标签草稿：添加 / 移除只改草稿，保存后才落库。
struct TaxonomyDraft {
    tags: Vec<String>,
}

impl NotesPage {
    /// 打开某条笔记的分类与标签编辑弹窗。
    ///
    /// 输入实体与草稿在打开时创建一次，闭包捕获后随弹窗关闭释放；保存
    /// 成功 / 失败均以 Toast 反馈，失败时保持弹窗打开以便重试。
    pub(super) fn open_taxonomy_dialog(
        &self,
        note: Note,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let note_id = note.id;
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
        let tag_colors = self.tag_colors.clone();
        let original_category = note.category.clone();
        let original_tags = note.tags.clone();
        let weak = cx.entity().downgrade();

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

        window.open_dialog(cx, move |dialog, _, _| {
            let category_input = category_input.clone();
            let tag_input = tag_input.clone();
            let draft = draft.clone();
            let tag_colors = tag_colors.clone();
            let save_for_ok = save.clone();
            let save_for_button = save.clone();
            dialog
                .title("分类与标签")
                .width(tokens::widths::DIALOG)
                .content(move |content, _, cx| {
                    let tags = draft.read(cx).tags.clone();
                    let theme = cx.theme().clone();
                    let muted = theme.muted_foreground;

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
                            .small()
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

                    content.child(
                        v_flex()
                            .gap_3()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_role(TextRole::Meta)
                                            .text_color(muted)
                                            .child("分类"),
                                    )
                                    .child(Input::new(&category_input).small().w_full()),
                            )
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_role(TextRole::Meta)
                                            .text_color(muted)
                                            .child("标签"),
                                    )
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(Input::new(&tag_input).small().w_full()),
                                            )
                                            .child(add_tag),
                                    )
                                    .child(tag_row),
                            ),
                    )
                })
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("taxonomy-cancel")
                                .outline()
                                .label("取消")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("taxonomy-save")
                                .primary()
                                .label("保存")
                                .on_click(move |_, window, cx| {
                                    if save_for_button(window, cx) {
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
                .on_ok(move |_, window, cx| save_for_ok(window, cx))
        });
    }
}
