//! 通用小部件（架构 §5.10）：标签徽片与分类标识。

use gpui_kit::assets::IconName;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{Icon, Sizable, Size};
use gpui_kit::{App, IntoElement, ParentElement, RenderOnce, SharedString, Window};

/// 标签徽片：outline 弱化样式 + 全圆角，用于卡片与筛选器展示单个标签。
#[derive(IntoElement)]
pub struct TagBadge {
    label: SharedString,
}

impl TagBadge {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl RenderOnce for TagBadge {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Tag::new()
            .outline()
            .with_size(Size::XSmall)
            .rounded_full()
            .child(self.label)
    }
}

/// 分类标识：实底次级样式 + 文件夹图标前缀，与标签徽片形成视觉区分。
#[derive(IntoElement)]
pub struct CategoryTag {
    label: SharedString,
}

impl CategoryTag {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl RenderOnce for CategoryTag {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Tag::new()
            .with_size(Size::XSmall)
            .rounded_full()
            .child(Icon::new(IconName::Folder).with_size(Size::XSmall))
            .child(self.label)
    }
}
