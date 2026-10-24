//! 通用小部件（架构 §5.10）：标签徽片与分类标识。
//!
//! 颜色取自条目的 DB 持久颜色（ADR-0008，`#RRGGBB` 落库）；未选色以
//! 主题中性样式渲染。分类徽片为填充 Tag（白字 + 文件夹图标），标签徽片
//! 为 outline Tag 带 `#` 前缀。chip 构造同时导出给分类标签表单复用。

use gpui_kit::assets::IconName;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable, Size, Theme};
use gpui_kit::{App, Hsla, IntoElement, ParentElement, RenderOnce, SharedString, Window};

use crate::utils::palette::filled_foreground;

/// 标签 chip（徽片与表单共用）：选色为 outline 描边样式（描边与文字同色），
/// 未选色为主题中性 outline（muted 前景 + 边框色）。
pub(crate) fn tag_outline_chip(
    label: impl Into<SharedString>,
    color: Option<Hsla>,
    theme: &Theme,
) -> Tag {
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

/// 分类 chip（徽片与表单共用）：选色为填充样式（色底白字），未选色为主题
/// 中性填充（secondary 底 + 常规前景）。
pub(crate) fn category_filled_chip(
    label: impl Into<SharedString>,
    color: Option<Hsla>,
    theme: &Theme,
) -> Tag {
    let tag = match color {
        Some(color) => Tag::custom(color, filled_foreground(), color),
        None => Tag::custom(theme.secondary, theme.foreground, theme.border),
    };
    tag.with_size(Size::XSmall)
        .rounded_full()
        .child(Icon::new(IconName::Folder).with_size(Size::XSmall))
        .child(label.into())
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

/// 分类标识：填充样式 + 文件夹图标前缀，与标签徽片形成视觉区分。
#[derive(IntoElement)]
pub struct CategoryTag {
    label: SharedString,
    color: Option<Hsla>,
}

impl CategoryTag {
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

impl RenderOnce for CategoryTag {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        category_filled_chip(self.label, self.color, cx.theme())
    }
}
