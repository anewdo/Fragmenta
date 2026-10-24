//! 分类与标签编辑表单（note-taxonomy-form spec）：Popover 弹层与表单内容。
//!
//! 表单为逐条即时保存语义（无整体确定 / 取消）：回车经 `mod.rs` 的
//! 会话订阅落库并清空输入框，点击已展示 Tag 移除对应数据，点击表单
//! 外部收起并结束会话。颜色取自页快照中的 DB 持久颜色（ADR-0008），
//! 未选色为中性样式；chip 构造与卡片徽片共用（widgets::tag_badge）。

use std::collections::HashMap;

use fragmenta_core::model::Note;

use gpui_kit::assets::IconName;
use gpui_kit::base::actions::Confirm;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::popover::{Popover, PopoverState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::NotesPage;
use crate::widgets::tag_badge::{category_filled_chip, tag_outline_chip};

/// 可点击 Tag：Tag 为纯展示组件，移除交互以可点击容器包裹
/// （hover 亮暗提示 + 点击回调）。
fn removable_tag(
    id: impl Into<ElementId>,
    tag: Tag,
    on_remove: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .hover(|style| style.opacity(0.7))
        .on_click(on_remove)
        .child(tag)
}

impl NotesPage {
    /// 卡片操作行的「分类与标签」按钮 + 锚定弹层。
    ///
    /// Popover 开关状态由 kit 内部管理：打开时经 `on_open_change` 创建
    /// 表单会话（全新输入框 + 回车订阅），关闭时释放；content 仅在会话
    /// 属于该卡片时挂载，展示数据为当前渲染帧的笔记快照（Changed 驱动
    /// 刷新后随页面重渲重建）。
    pub(super) fn taxonomy_popover(&self, note: &Note, weak: &WeakEntity<Self>) -> Popover {
        let note_id = note.id;
        let weak_toggle = weak.clone();
        let mut popover = Popover::new(("note-taxonomy", note_id as u64))
            .trigger(
                Button::new(("note-taxonomy-btn", note_id as u64))
                    .ghost()
                    .compact()
                    .xsmall()
                    .icon(IconName::Tags),
            )
            .on_open_change(move |open, window, cx| {
                let _ = weak_toggle.update(cx, |this, cx| {
                    if *open {
                        this.open_taxonomy_form(note_id, window, cx);
                    } else {
                        this.close_taxonomy_form(note_id, cx);
                    }
                });
            });
        if let Some(form) = self
            .taxonomy_form
            .as_ref()
            .filter(|form| form.note_id == note_id)
        {
            let category_input = form.category_input.clone();
            let tag_input = form.tag_input.clone();
            let category = note.category.clone();
            let tags = note.tags.clone();
            // 颜色快照随会话克隆：content 每帧调用，闭包内不可查询
            let category_colors = self.category_colors.clone();
            let tag_colors = self.tag_colors.clone();
            let weak_content = weak.clone();
            popover = popover.content(move |_, _, cx| {
                taxonomy_form_content(
                    note_id,
                    &category_input,
                    &tag_input,
                    category.clone(),
                    &tags,
                    &category_colors,
                    &tag_colors,
                    &weak_content,
                    cx,
                )
            });
        }
        popover
    }
}

/// 表单内容：分类 / 标签两个输入区（label + 输入框 + 可点击 Tag 行）。
///
/// 每帧随 Popover 重绘调用：仅做纯构建（输入实体、笔记数据与颜色映射
/// 均为快照），取色走页快照中的 DB 持久颜色（未选色为中性样式）。
#[allow(clippy::too_many_arguments)]
fn taxonomy_form_content(
    note_id: i64,
    category_input: &Entity<InputState>,
    tag_input: &Entity<InputState>,
    category: Option<String>,
    tags: &[String],
    category_colors: &HashMap<String, Option<Hsla>>,
    tag_colors: &HashMap<String, Option<Hsla>>,
    weak: &WeakEntity<NotesPage>,
    cx: &mut Context<PopoverState>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let theme = cx.theme().clone();

    // 分类：单值，填充 chip，点击清除
    let category_row = match category {
        Some(category) => {
            let color = category_colors.get(&category).copied().flatten();
            let weak_remove = weak.clone();
            removable_tag(
                ("taxonomy-category-tag", note_id as u64),
                category_filled_chip(category.clone(), color, &theme),
                move |_, _, cx| {
                    let _ = weak_remove
                        .update(cx, |this, cx| this.remove_taxonomy_category(note_id, cx));
                },
            )
            .into_any_element()
        }
        None => div()
            .text_size(px(11.))
            .text_color(muted)
            .child("暂无分类，输入后回车添加")
            .into_any_element(),
    };

    // 标签：多值，outline chip 带 # 前缀，点击移除该标签
    let tag_row = if tags.is_empty() {
        div()
            .text_size(px(11.))
            .text_color(muted)
            .child("暂无标签，输入后回车添加")
            .into_any_element()
    } else {
        h_flex()
            .flex_wrap()
            .gap_1()
            .children(tags.iter().enumerate().map(|(ix, tag)| {
                let tag = tag.clone();
                let color = tag_colors.get(&tag).copied().flatten();
                let weak_remove = weak.clone();
                removable_tag(
                    ("taxonomy-tag", ix),
                    tag_outline_chip(tag.clone(), color, &theme),
                    move |_, _, cx| {
                        let _ = weak_remove
                            .update(cx, |this, cx| this.remove_taxonomy_tag(note_id, &tag, cx));
                    },
                )
            }))
            .into_any_element()
    };

    // 表单容器拦截 Confirm：Popover 全局注册了 enter → Confirm（开关弹层），
    // 而 Input 单行回车会 propagate 外传——若不拦截，回车先经 Confirm 关闭
    // 表单（会话释放、订阅退订），PressEnter 事件随后才异步分发而无人处理，
    // 造成"回车即关闭且不保存"。拦截后表单内回车仅走保存语义。
    // Escape（Cancel）与点击外部不受影响。
    v_flex()
        .w(px(300.))
        .gap_3()
        .on_action(|_: &Confirm, _, cx| cx.stop_propagation())
        .child(
            v_flex()
                .gap_1()
                .child(div().text_size(px(12.)).text_color(muted).child("分类"))
                .child(Input::new(category_input).small().w_full())
                .child(category_row),
        )
        .child(
            v_flex()
                .gap_1()
                .child(div().text_size(px(12.)).text_color(muted).child("标签"))
                .child(Input::new(tag_input).small().w_full())
                .child(tag_row),
        )
        .into_any_element()
}
