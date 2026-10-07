//! 首页（Phase 12 实装：月历 + 选中日待办列表）。Phase 7 交付空白占位页。

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 首页视图：Phase 7 空白占位（仅页面名标识），常驻于主窗口 shell。
pub struct HomePage;

impl Render for HomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex().size_full().items_center().justify_center().child(
            div()
                .text_size(px(20.))
                .text_color(cx.theme().foreground.opacity(0.4))
                .child("首页"),
        )
    }
}
