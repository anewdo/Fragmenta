//! 笔记列表文本排版：笔记页卡片与磁贴列表共用标题 / 摘要截断规则。

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::tokens::TextRole;

/// 笔记列表标题：单行截断（超宽以省略号收尾），字重与行高取文档标题角色。
/// 返回元素可伸缩，放在水平容器里与右侧元信息共用一行。
pub fn note_title(title: impl Into<SharedString>, font_px: Pixels) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .line_clamp(1)
        .text_ellipsis()
        .text_size(font_px)
        .font_weight(TextRole::DocumentTitle.weight())
        .line_height(TextRole::DocumentTitle.line_height())
        .child(title.into())
}

/// 笔记列表摘要：最多 `lines` 行，超出以省略号截断；颜色由调用方决定。
pub fn note_summary(
    summary: impl Into<SharedString>,
    lines: usize,
    font_px: Pixels,
    color: Hsla,
) -> Div {
    div()
        .w_full()
        .text_size(font_px)
        .line_height(TextRole::Reading.line_height())
        .text_color(color)
        .line_clamp(lines)
        .text_ellipsis()
        .child(summary.into())
}
