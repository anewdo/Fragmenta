//! 笔记页（Phase 9 左栏列表 / Phase 10 编辑器 + 双栏组装）。Phase 7 交付空白占位页。

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 笔记页视图：Phase 7 空白占位（仅页面名标识），常驻于主窗口 shell。
pub struct NotesPage;

impl Render for NotesPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex().size_full().items_center().justify_center().child(
            div()
                .text_size(px(20.))
                .text_color(cx.theme().foreground.opacity(0.4))
                .child("笔记"),
        )
    }
}
