//! 设置页：Phase 13 实装主题 / 字号 / 热键 / 数据迁移 / 导出；
//! Phase 8 起提供常驻进程的显式退出入口（spike ⑤ 定案的退出路径归宿）。

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::NotesStore;

/// 设置页视图：常驻于主窗口 shell。
pub struct SettingsPage;

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(
                div()
                    .text_size(px(20.))
                    .text_color(cx.theme().foreground.opacity(0.4))
                    .child("设置"),
            )
            .child(
                Button::new("quit")
                    .label("退出 Fragmenta")
                    .danger()
                    .on_click(|_, _, cx| {
                        // 退出前把全部 pending 防抖内容同步落库，避免丢失最后 300ms 输入
                        let notes = cx.global::<NotesStore>().0.clone();
                        notes.update(cx, |state, cx| state.flush_all(cx));
                        cx.quit();
                    }),
            )
    }
}
