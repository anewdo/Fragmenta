//! 设置页（架构 §5.8 / §5.11）：Phase 13 分组实装，kit `Settings` 组件承载。
//!
//! 本阶段交付"系统"组：关闭主窗口时直接退出（默认关 = 托盘驻留）与
//! 显式退出按钮（Phase 8 占位移入，spike ⑤ 定案的退出路径归宿）。
//! 外观 / 快捷键 / 数据组由后续 ticket 落地。

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings};
use gpui_kit::component::{Sizable, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::{SettingsStore, quit_flushing};

/// 设置页视图：常驻于主窗口 shell。
pub struct SettingsPage;

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_6()
            .child(Settings::new("fragmenta-settings").pages(vec![SettingPage::new("设置")
                .default_open(true)
                .group(
                    SettingGroup::new()
                        .title("系统")
                        .items(vec![
                            SettingItem::new(
                                "关闭主窗口时直接退出",
                                SettingField::switch(
                                    |cx: &App| {
                                        cx.global::<SettingsStore>()
                                            .0
                                            .read(cx)
                                            .close_to_quit()
                                    },
                                    |val: bool, cx: &mut App| {
                                        let settings = cx.global::<SettingsStore>().0.clone();
                                        settings.update(cx, |state, cx| {
                                            state.set_close_to_quit(val, cx)
                                        });
                                    },
                                )
                                .default_value(false),
                            )
                            .description("默认关闭：关闭主窗口后驻留系统托盘，热键呼出磁贴不受影响"),
                            SettingItem::new(
                                "退出应用",
                                SettingField::render(|options, _window, _cx| {
                                    Button::new("quit")
                                        .label("退出 Fragmenta")
                                        .danger()
                                        .with_size(options.size())
                                        .on_click(|_, _, cx| quit_flushing(cx))
                                }),
                            )
                            .description("退出前未保存内容自动落库"),
                        ]),
                )]))
    }
}
