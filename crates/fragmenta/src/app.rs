//! 启动编排：assets、主题、主窗口。
//!
//! 全量图标、按 `Settings` 初始化主题、启动即打开主窗口；
//! NotesState、磁贴热键与进程常驻在 Phase 8 接入。

use std::path::PathBuf;

use fragmenta_core::settings::{FontScale, Settings, ThemeMode};
use gpui_kit::component::{Theme, ThemeMode as KitThemeMode};
use gpui_kit::{App, WindowAppearance, px};

use crate::windows;

/// settings.json 固定位于 exe 旁
fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("settings.json")
}

/// 应用入口；`main.rs` 仅调用此函数。
pub fn run() {
    let settings = Settings::load(&settings_path()).unwrap_or_else(|err| {
        eprintln!("[fragmenta] 读取设置失败，使用默认设置: {err}");
        Settings::default()
    });
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets::new(""))
        .run(move |cx| {
            gpui_kit::init(cx);
            apply_theme(&settings, cx);
            windows::main::open(cx);
        });
}

/// 按 `Settings` 初始化主题：模式（Auto 解析为当前系统外观，桥接收敛于
/// Phase 13 SettingsState）与字号档位（Medium 即 kit 默认 16px）。
fn apply_theme(settings: &Settings, cx: &mut App) {
    let mode = match settings.theme {
        ThemeMode::Light => KitThemeMode::Light,
        ThemeMode::Dark => KitThemeMode::Dark,
        ThemeMode::Auto => match cx.window_appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => KitThemeMode::Dark,
            _ => KitThemeMode::Light,
        },
    };
    Theme::change(mode, None, cx);
    let font_size = match settings.font_scale {
        FontScale::Small => px(14.),
        FontScale::Medium => px(16.),
        FontScale::Large => px(18.),
    };
    Theme::update(cx, |theme| theme.font_size = font_size);
}
