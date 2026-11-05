//! 设置页共用骨架：分组标题、卡片标题与设置行。

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::layout::section_title;
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};

/// 设置分组：图标 + 区块标题 + 下方子项列表，组与组之间由外层更大间距分隔。
pub(super) fn setting_section(
    icon: IconName,
    title: &'static str,
    foreground: Hsla,
    items: impl IntoElement,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(tokens::spacing::MD)
        .child(
            h_flex()
                .gap(tokens::spacing::SM)
                .child(Icon::new(icon).size_4().text_color(foreground))
                .child(section_title(title, foreground)),
        )
        .child(items)
}

/// 卡片标题：图标 + 标签，下方为说明文字。
pub(super) fn card_header(
    icon: IconName,
    title: &'static str,
    description: impl IntoElement,
    muted: Hsla,
) -> impl IntoElement {
    v_flex()
        .gap(tokens::spacing::XS)
        .child(
            h_flex()
                .gap(tokens::spacing::SM)
                .child(Icon::new(icon).size_4().text_color(muted))
                .child(div().text_role(TextRole::ControlLabel).child(title)),
        )
        .child(
            div()
                .text_role(TextRole::Meta)
                .text_color(muted)
                .child(description),
        )
}

/// 设置行标题：图标 + 标签文字。
pub(super) fn row_title(icon: IconName, title: &'static str, muted: Hsla) -> impl IntoElement {
    h_flex()
        .gap(tokens::spacing::SM)
        .child(Icon::new(icon).size_4().text_color(muted))
        .child(div().text_role(TextRole::ControlLabel).child(title))
}

/// 设置行布局：左侧图标 + 标题与说明，右侧控件。
pub(super) fn setting_row(
    icon: IconName,
    title: &'static str,
    description: impl IntoElement,
    muted: Hsla,
    field: impl IntoElement,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(tokens::spacing::LG)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(tokens::spacing::XS)
                .child(row_title(icon, title, muted))
                .child(
                    div()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .child(description),
                ),
        )
        .child(field)
}

/// 设置项错误提示：固定元信息字号 + 危险色，右对齐展示。
pub(super) fn error_text(message: impl IntoElement, danger: Hsla) -> impl IntoElement {
    div()
        .text_role(TextRole::Meta)
        .text_color(danger)
        .child(message)
}
