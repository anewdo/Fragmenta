//! SettingsState headless 测试：主题 / 字号修改 → settings.json 真实落盘 +
//! 广播 + kit 主题即时切换（真实文件系统，无 mock 无桩，架构 §5.6 / D8）。

mod common;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use common::TempDir;
use fragmenta::settings::{SettingsEvent, SettingsState};
use fragmenta_core::settings::{Settings, ThemeMode};
use gpui_kit::component::{ActiveTheme as _, ThemeMode as KitThemeMode};
use gpui_kit::{AppContext as _, Entity, TestAppContext, px, rgb};

/// 订阅设置广播并收集为序列。
fn track_events(
    state: &Entity<SettingsState>,
    cx: &TestAppContext,
) -> Rc<RefCell<Vec<SettingsEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|cx| {
        cx.subscribe(state, move |_, event: &SettingsEvent, _| {
            sink.borrow_mut().push(*event);
        })
        .detach();
    });
    events
}

/// 构造 SettingsState；测试环境先初始化 kit 主题全局（生产由 gpui_kit::init 完成）。
/// 共享数据库用内存库（本文件不触迁移路径）。
fn new_state(settings: Settings, path: &Path, cx: &mut TestAppContext) -> Entity<SettingsState> {
    cx.update(gpui_kit::component::theme::init);
    let path = path.to_path_buf();
    let db = std::sync::Arc::new(fragmenta_core::db::Db::open_in_memory().unwrap());
    cx.new(|cx| SettingsState::new(settings, path, db, cx))
}

/// set_theme 切换 Dark：落盘 + 广播一次 + kit 主题即时变暗（重启读路径保留）。
#[gpui_kit::test]
fn set_theme_persists_broadcasts_and_applies(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-state");
    let path = dir.path().join("settings.json");
    let state = new_state(Settings::default(), &path, cx);
    let events = track_events(&state, cx);

    state.update(cx, |state, cx| state.set_theme(ThemeMode::Dark, cx));

    let theme = state.read_with(cx, |state, _| state.theme());
    assert_eq!(theme, ThemeMode::Dark);
    assert_eq!(*events.borrow(), vec![SettingsEvent::Changed]);
    assert_eq!(cx.update(|cx| cx.theme().mode), KitThemeMode::Dark);
    assert_eq!(Settings::load(&path).unwrap().theme, ThemeMode::Dark);
}

/// set_theme 切换到色系主题：以浅色模式生效、落盘 + 广播。
#[gpui_kit::test]
fn set_theme_color_theme_applies_as_light(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-state-color-theme");
    let path = dir.path().join("settings.json");
    let state = new_state(Settings::default(), &path, cx);
    let events = track_events(&state, cx);

    state.update(cx, |state, cx| {
        state.set_theme(ThemeMode::LiteraryGreen, cx)
    });

    let theme = state.read_with(cx, |state, _| state.theme());
    assert_eq!(theme, ThemeMode::LiteraryGreen);
    assert_eq!(*events.borrow(), vec![SettingsEvent::Changed]);
    cx.update(|cx| {
        assert_eq!(cx.theme().mode, KitThemeMode::Light);
        assert_eq!(cx.theme().background, rgb(0xF4F1EA).into());
    });
    assert_eq!(
        Settings::load(&path).unwrap().theme,
        ThemeMode::LiteraryGreen
    );
}

/// set_editor_font_px：落盘 + 广播一次 + 访问器像素映射（重启读路径保留）。
#[gpui_kit::test]
fn set_editor_font_px_persists_broadcasts_and_applies(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-state");
    let path = dir.path().join("settings.json");
    let state = new_state(Settings::default(), &path, cx);
    let events = track_events(&state, cx);

    state.update(cx, |state, cx| state.set_editor_font_px(px(20.), cx));

    let font_px = state.read_with(cx, |state, _| state.editor_font_px());
    assert_eq!(font_px, px(20.));
    assert_eq!(*events.borrow(), vec![SettingsEvent::Changed]);
    assert_eq!(Settings::load(&path).unwrap().editor_font_px, 20.);
}

/// set_tile_font_px：落盘 + 广播一次；超出可调范围钳制到上限。
#[gpui_kit::test]
fn set_tile_font_px_persists_and_clamps(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-state");
    let path = dir.path().join("settings.json");
    let state = new_state(Settings::default(), &path, cx);
    let events = track_events(&state, cx);

    state.update(cx, |state, cx| state.set_tile_font_px(px(99.), cx));

    let font_px = state.read_with(cx, |state, _| state.tile_font_px());
    assert_eq!(font_px, px(fragmenta_core::settings::FONT_SIZE_MAX_PX));
    assert_eq!(*events.borrow(), vec![SettingsEvent::Changed]);
    assert_eq!(
        Settings::load(&path).unwrap().tile_font_px,
        fragmenta_core::settings::FONT_SIZE_MAX_PX
    );
}

/// 修改相同字号为无操作：不广播、不落盘。
#[gpui_kit::test]
fn unchanged_value_is_noop(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-state");
    let path = dir.path().join("settings.json");
    let state = new_state(Settings::default(), &path, cx);
    let events = track_events(&state, cx);

    state.update(cx, |state, cx| state.set_theme(ThemeMode::Light, cx));
    state.update(cx, |state, cx| {
        state.set_editor_font_px(px(fragmenta_core::settings::FONT_SIZE_DEFAULT_PX), cx)
    });
    state.update(cx, |state, cx| {
        state.set_tile_font_px(px(fragmenta_core::settings::FONT_SIZE_DEFAULT_PX), cx)
    });

    assert!(events.borrow().is_empty());
    assert!(!path.exists());
}

/// set_hotkey 成功路径：格式校验通过后落盘 + 广播 + 读取返回新值
/// （headless 无热键控制器，即启动期注册失败的降级路径：跳过注册，
/// 重启后新热键生效）。
#[gpui_kit::test]
fn set_hotkey_persists_and_broadcasts(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-state");
    let path = dir.path().join("settings.json");
    let state = new_state(Settings::default(), &path, cx);
    let events = track_events(&state, cx);

    state
        .update(cx, |state, cx| state.set_hotkey("ctrl+alt+t", cx))
        .unwrap();

    assert_eq!(
        state.read_with(cx, |state, _| state.tile_hotkey().to_string()),
        "ctrl+alt+t",
    );
    assert_eq!(*events.borrow(), vec![SettingsEvent::Changed]);
    assert_eq!(Settings::load(&path).unwrap().tile_hotkey, "ctrl+alt+t");
}

/// set_hotkey 拒绝纯修饰键组合：返回 Err、不落库、不广播，旧热键保留。
#[gpui_kit::test]
fn set_hotkey_rejects_pure_modifiers(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-state");
    let path = dir.path().join("settings.json");
    let state = new_state(Settings::default(), &path, cx);
    let events = track_events(&state, cx);

    state
        .update(cx, |state, cx| state.set_hotkey("ctrl+alt", cx))
        .unwrap_err();

    assert_eq!(
        state.read_with(cx, |state, _| state.tile_hotkey().to_string()),
        Settings::default().tile_hotkey,
    );
    assert!(events.borrow().is_empty());
    assert!(!path.exists());
}

/// set_hotkey 相同热键为无操作：不广播、不落盘。
#[gpui_kit::test]
fn set_hotkey_unchanged_is_noop(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-state");
    let path = dir.path().join("settings.json");
    let state = new_state(Settings::default(), &path, cx);
    let events = track_events(&state, cx);

    state
        .update(cx, |state, cx| {
            state.set_hotkey(&Settings::default().tile_hotkey, cx)
        })
        .unwrap();

    assert!(events.borrow().is_empty());
    assert!(!path.exists());
}
