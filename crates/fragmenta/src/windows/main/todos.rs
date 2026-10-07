//! 待办页（Phase 11 实装：TodosState + 按日期组织的增删改与勾选）。Phase 7 交付空白占位页。

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 待办页视图：Phase 7 空白占位（仅页面名标识），常驻于主窗口 shell。
pub struct TodosPage;

impl Render for TodosPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex().size_full().items_center().justify_center().child(
            div()
                .text_size(px(20.))
                .text_color(cx.theme().foreground.opacity(0.4))
                .child("待办"),
        )
    }
}
