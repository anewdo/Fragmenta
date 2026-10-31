//! 笔记标签徽片：仅笔记卡片展示使用。
//!
//! 颜色取自条目的 DB 持久颜色（ADR-0008，`#RRGGBB` 落库）；未选色以
//! 主题中性样式渲染。标签徽片为 outline Tag 带 `#` 前缀。

use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Sizable, Size, Theme};
use gpui_kit::{App, Hsla, IntoElement, ParentElement, RenderOnce, SharedString, Window};

/// 标签 chip 的 outline 样式：选色时描边与文字同色，未选色为主题中性。
fn tag_outline_chip(label: impl Into<SharedString>, color: Option<Hsla>, theme: &Theme) -> Tag {
    let text = format!("#{}", label.into());
    let tag = match color {
        Some(color) => Tag::custom(color, color, color),
        None => Tag::custom(theme.secondary, theme.muted_foreground, theme.border),
    };
    tag.outline()
        .with_size(Size::XSmall)
        .rounded_full()
        .child(text)
}

/// 标签徽片：outline 描边样式 + `#` 前缀，用于卡片展示单个标签。
#[derive(IntoElement)]
pub struct TagBadge {
    label: SharedString,
    color: Option<Hsla>,
}

impl TagBadge {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            color: None,
        }
    }

    /// 展示颜色；`None` 为未选色（主题中性样式）。
    pub fn color(mut self, color: Option<Hsla>) -> Self {
        self.color = color;
        self
    }
}

impl RenderOnce for TagBadge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        tag_outline_chip(self.label, self.color, cx.theme())
    }
}
