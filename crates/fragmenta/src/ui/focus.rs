//! 文本输入聚焦视觉（ADR-0010）：关闭 kit 焦点环后统一用 1px 主色描边。

use gpui_kit::component::FocusableExt as _;
use gpui_kit::component::Theme;
use gpui_kit::component::input::Input;
use gpui_kit::component::time_field::TimeField;
use gpui_kit::{Hsla, Styled};

/// 聚焦描边不透明度：与笔记搜索框一致，避免粗重的环与阴影。
pub const FOCUS_BORDER_OPACITY: f32 = 0.6;

/// 文本输入边框色：聚焦时主色半透明，否则主题输入描边。
pub fn input_border(theme: &Theme, focused: bool) -> Hsla {
    if focused {
        theme.primary.opacity(FOCUS_BORDER_OPACITY)
    } else {
        theme.input
    }
}

/// 为 kit 文本输入套用统一聚焦描边；`border` 由 [`input_border`] 计算。
pub trait TextFieldExt: Sized {
    fn text_field_focus(self, border: Hsla) -> Self;
}

impl TextFieldExt for Input {
    fn text_field_focus(self, border: Hsla) -> Self {
        self.focus_ring(false).border_color(border)
    }
}

impl TextFieldExt for TimeField {
    fn text_field_focus(self, border: Hsla) -> Self {
        self.border_color(border)
    }
}
