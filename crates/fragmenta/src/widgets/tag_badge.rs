//! 通用小部件（架构 §5.10）：标签徽片与分类标识。
//!
//! 颜色取自全局取色盘（note-taxonomy-form）：与分类标签表单共用同一
//! 名字 → 颜色映射，同屏同名同色。分类徽片为填充色 Tag（白字 + 文件夹
//! 图标），标签徽片为 outline Tag 带 `#` 前缀。

use gpui_kit::assets::IconName;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{Icon, Sizable, Size};
use gpui_kit::{App, Hsla, IntoElement, ParentElement, RenderOnce, SharedString, Window};

use crate::app::PaletteStore;
use crate::utils::palette::filled_foreground;

/// 从全局取色盘按名字取色（映射进程内稳定，同名同色）。
fn color_of(name: &SharedString, cx: &App) -> Hsla {
    let mut palette = cx.global::<PaletteStore>().0.lock().expect("取色盘锁中毒");
    palette.color_for(name.as_ref())
}

/// 标签徽片：outline 描边样式 + `#` 前缀，用于卡片与筛选器展示单个标签。
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = color_of(&self.label, cx);
        Tag::custom(color, color, color)
            .outline()
            .with_size(Size::XSmall)
            .rounded_full()
            .child(format!("#{}", self.label))
    }
}

/// 分类标识：填充色样式 + 文件夹图标前缀，与标签徽片形成视觉区分。
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = color_of(&self.label, cx);
        Tag::custom(color, filled_foreground(), color)
            .with_size(Size::XSmall)
            .rounded_full()
            .child(Icon::new(IconName::Folder).with_size(Size::XSmall))
            .child(self.label)
    }
}
