//! 设置页（架构 §5.8 / §5.11）：Phase 13 分组实装（GroupBox 分组卡片）。
//!
//! 本阶段交付"系统"组：关闭主窗口时直接退出（默认关 = 托盘驻留）。
//! 退出入口收敛至托盘菜单（用户裁决：设置页不再保留退出按钮）。
//! 外观 / 快捷键 / 数据组由后续 ticket 落地。

use gpui_kit::component::group_box::GroupBox;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::SettingsStore;

/// 设置页视图：常驻于主窗口 shell。
pub struct SettingsPage;

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let close_to_quit = cx.global::<SettingsStore>().0.read(cx).close_to_quit();
        let muted = cx.theme().muted_foreground;
        v_flex()
            .size_full()
            .items_center()
            .overflow_y_scrollbar()
            .p_6()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(560.))
                    .gap_4()
                    .child(
                        div()
                            .text_size(px(20.))
                            .text_color(cx.theme().foreground.opacity(0.4))
                            .child("设置"),
                    )
                    .child(
                        GroupBox::new()
                            .title("系统")
                            .child(system_group(close_to_quit, muted)),
                    ),
            )
    }
}

/// "系统"组：关闭主窗口时直接退出开关。
fn system_group(close_to_quit: bool, muted: Hsla) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            v_flex()
                .gap_1()
                .child(div().child("关闭主窗口时直接退出"))
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("默认关闭：关闭主窗口后驻留系统托盘，热键呼出磁贴不受影响"),
                ),
        )
        .child(
            Switch::new("close-to-quit")
                .checked(close_to_quit)
                .on_click(|checked: &bool, _window, cx| {
                    let settings = cx.global::<SettingsStore>().0.clone();
                    settings.update(cx, |state, cx| state.set_close_to_quit(*checked, cx));
                }),
        )
}
