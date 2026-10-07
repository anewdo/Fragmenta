//! 设置页（Phase 13 实装：主题 / 字号 / 热键 / 数据迁移 / 导出）。Phase 7 交付空白占位页。

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 设置页视图：Phase 7 空白占位（仅页面名标识），常驻于主窗口 shell。
pub struct SettingsPage;

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex().size_full().items_center().justify_center().child(
            div()
                .text_size(px(20.))
                .text_color(cx.theme().foreground.opacity(0.4))
                .child("设置"),
        )
    }
}
