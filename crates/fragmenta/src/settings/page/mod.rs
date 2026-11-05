//! 设置页：常驻实体装配与页面骨架；各组内容与控件状态见同目录子模块。

mod appearance;
mod common;
mod data;
mod font_choices;
mod font_picker;
mod hotkey;
mod system;
mod typography;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::{SettingsEvent, SettingsStore};
use crate::ui::layout::{LayoutClass, content_column, page_scroll};
use crate::ui::tokens;

use common::setting_section;
use hotkey::HotkeyRecorder;
use typography::TypographyControls;

/// 设置页视图：常驻于主窗口 shell，订阅设置广播刷新展示。
pub(crate) struct SettingsPage {
    _subscription: Subscription,
    /// 窗口尺寸变化：档位留白随视口宽度切换。
    _bounds_sub: Subscription,
    /// 编辑器 / 小窗两套排版控件。
    typography: TypographyControls,
    /// 热键录入会话状态。
    hotkey: HotkeyRecorder,
    /// 数据目录迁移错误提示（红字展示）。
    data_dir_error: Option<String>,
    /// 开机自启动开关错误提示（红字展示）。
    autostart_error: Option<String>,
}

impl SettingsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = cx.global::<SettingsStore>().0.clone();
        let subscription = cx.subscribe_in(
            &settings,
            window,
            |this, _, _: &SettingsEvent, window, cx| {
                // 设置变更（含本页操作）→ 字体候选可能与自定义列表相关，先同步下拉
                this.typography.sync(window, cx);
                cx.notify();
            },
        );
        let bounds_sub = cx.observe_window_bounds(window, |_, _, cx| cx.notify());
        Self {
            _subscription: subscription,
            _bounds_sub: bounds_sub,
            typography: TypographyControls::new(window, cx),
            hotkey: HotkeyRecorder::new(cx),
            data_dir_error: None,
            autostart_error: None,
        }
    }
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let foreground = cx.theme().foreground;
        let content = content_column(tokens::widths::SETTINGS_CONTENT)
            .gap(tokens::spacing::XXL)
            .child(setting_section(
                IconName::SwatchBook,
                "外观",
                foreground,
                self.appearance_group(cx),
            ))
            .child(setting_section(
                IconName::Keyboard,
                "快捷键",
                foreground,
                self.hotkey_group(cx),
            ))
            .child(setting_section(
                IconName::Database,
                "数据",
                foreground,
                self.data_group(cx),
            ))
            .child(setting_section(
                IconName::Settings2,
                "系统",
                foreground,
                self.system_group(cx),
            ));
        v_flex()
            .size_full()
            .child(page_scroll(
                LayoutClass::from_width(window.viewport_size().width),
                content,
            ))
            .when_some(self.typography.font_picker.entity(), |this, picker| {
                this.child(picker)
            })
    }
}
