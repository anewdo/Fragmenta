//! 页面骨架：布局档位、内容栏、区块标题与设置卡片（ADR-0007）。

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Theme, v_flex};
use gpui_kit::*;

use crate::ui::tokens::{self, TextRole, TextRoleExt as _};

/// 视口宽度档位；断点只影响留白与首次进入的紧凑布局，不强制改写用户状态。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutClass {
    Compact,
    Regular,
    Wide,
}

impl LayoutClass {
    pub fn from_width(width: Pixels) -> Self {
        if width < tokens::breakpoints::COMPACT_MAX {
            Self::Compact
        } else if width >= tokens::breakpoints::WIDE_MIN {
            Self::Wide
        } else {
            Self::Regular
        }
    }

    /// 页面四周留白：Compact 16 / Regular 24 / Wide 32。
    pub fn outer_padding(self) -> Pixels {
        match self {
            Self::Compact => tokens::spacing::LG,
            Self::Regular => tokens::spacing::XL,
            Self::Wide => tokens::spacing::XXL,
        }
    }
}

/// 页面外框：铺满可滚动区、按档位留白，主内容水平居中。
pub fn page_scroll(layout: LayoutClass, content: impl IntoElement) -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .overflow_y_scrollbar()
        .p(layout.outer_padding())
        .child(content)
}

/// 内容栏：受最大宽度约束的主内容列；窄窗口下随外框收缩。
pub fn content_column(max_width: Pixels) -> Div {
    v_flex().w_full().max_w(max_width)
}

/// 区块标题：固定 18px 半粗，颜色取前景色。
pub fn section_title(title: impl IntoElement, foreground: Hsla) -> impl IntoElement {
    div()
        .text_role(TextRole::SectionTitle)
        .text_color(foreground)
        .child(title)
}

/// 设置子项卡片的配色与圆角。
#[derive(Clone, Copy)]
pub struct CardStyle {
    pub bg: Hsla,
    pub border: Hsla,
    pub radius: Pixels,
}

impl CardStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            bg: theme.group_box,
            border: theme.border,
            radius: tokens::radius::SM,
        }
    }
}

/// 单行设置卡片：淡底色 + 描边 + 圆角。
pub fn setting_card(style: CardStyle, content: impl IntoElement) -> impl IntoElement {
    div()
        .w_full()
        .px(tokens::spacing::LG)
        .py(tokens::spacing::MD)
        .rounded(style.radius)
        .bg(style.bg)
        .border_1()
        .border_color(style.border)
        .child(content)
}
