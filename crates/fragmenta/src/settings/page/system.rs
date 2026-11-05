//! 系统组：关闭主窗口语义与开机自启动开关。

use gpui_kit::assets::IconName;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;

use super::SettingsPage;
use super::common::{error_text, setting_row};
use crate::settings::SettingsStore;
use crate::ui::layout::{CardStyle, setting_card};
use crate::ui::tokens;

impl SettingsPage {
    /// "系统"组：关闭主窗口语义开关、开机自启动开关
    /// （注册表操作失败红字展示，开关状态不前移）。
    pub(super) fn system_group(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<SettingsStore>().0.read(cx);
        let (close_to_quit, autostart) = (settings.close_to_quit(), settings.autostart());
        let card = CardStyle::from_theme(cx.theme());
        let (muted, danger) = {
            let colors = cx.theme();
            (colors.muted_foreground, colors.danger)
        };
        let error = self.autostart_error.clone();

        v_flex()
            .w_full()
            .gap(tokens::spacing::SM)
            .child(setting_card(
                card,
                setting_row(
                    IconName::LogOut,
                    "关闭主窗口时直接退出",
                    "默认关闭：关闭主窗口后驻留系统托盘，热键呼出磁贴不受影响",
                    muted,
                    Switch::new("close-to-quit")
                        .checked(close_to_quit)
                        .on_click(|checked: &bool, _window, cx| {
                            let settings = cx.global::<SettingsStore>().0.clone();
                            settings.update(cx, |state, cx| state.set_close_to_quit(*checked, cx));
                        }),
                ),
            ))
            .child(setting_card(
                card,
                setting_row(
                    IconName::Power,
                    "开机自启动",
                    "登录 Windows 后静默启动并驻留托盘，不打开主窗口",
                    muted,
                    Switch::new("autostart")
                        .checked(autostart)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            let store = cx.global::<SettingsStore>().0.clone();
                            match store.update(cx, |state, cx| state.set_autostart(*checked, cx)) {
                                Ok(()) => this.autostart_error = None,
                                Err(err) => this.autostart_error = Some(err.to_string()),
                            }
                            cx.notify();
                        })),
                ),
            ))
            .when_some(error, |el, err| {
                el.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .child(error_text(err, danger)),
                )
            })
    }
}
