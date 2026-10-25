//! Settings 持久化测试：字段默认值、往返与既有 JSON 兼容。

mod common;

use fragmenta_core::settings::{FONT_SIZE_DEFAULT_PX, FONT_SIZE_MAX_PX, Settings};

#[test]
fn font_sizes_default_roundtrip_and_clamp() {
    // 两处字号默认 16px；保存后重读保留；手动改成越界值时读取钳制
    let dir = common::TempDir::new("settings-font-size");
    let path = dir.path().join("settings.json");
    let settings = Settings {
        editor_font_px: 18.,
        tile_font_px: 20.,
        ..Default::default()
    };
    settings.save(&path).expect("保存失败");
    let loaded = Settings::load(&path).expect("读取失败");
    assert_eq!(loaded.editor_font_px, 18.);
    assert_eq!(loaded.tile_font_px, 20.);

    let default = Settings::default();
    assert_eq!(default.editor_font_px, FONT_SIZE_DEFAULT_PX);
    assert_eq!(default.tile_font_px, FONT_SIZE_DEFAULT_PX);

    std::fs::write(
        &path,
        r#"{"editor_font_px":99,"tile_font_px":-4,"data_dir":"C:/x"}"#,
    )
    .expect("写入失败");
    let loaded = Settings::load(&path).expect("读取失败");
    assert_eq!(loaded.editor_font_px, FONT_SIZE_MAX_PX);
    assert_eq!(
        loaded.tile_font_px,
        fragmenta_core::settings::FONT_SIZE_MIN_PX
    );
}

#[test]
fn close_to_quit_roundtrip() {
    // close_to_quit = true 保存后重读保留
    let dir = common::TempDir::new("settings-close-quit");
    let path = dir.path().join("settings.json");
    let settings = Settings {
        close_to_quit: true,
        ..Default::default()
    };
    settings.save(&path).expect("保存失败");
    let loaded = Settings::load(&path).expect("读取失败");
    assert!(loaded.close_to_quit);
}

#[test]
fn missing_close_to_quit_defaults_false() {
    // 既有 settings.json（无 close_to_quit 字段）容错读取，默认 false
    let dir = common::TempDir::new("settings-legacy");
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"theme":"Dark","font_scale":"Large","data_dir":"C:/x","tile_hotkey":"ctrl+alt+n"}"#,
    )
    .expect("写入失败");
    let loaded = Settings::load(&path).expect("读取失败");
    assert!(!loaded.close_to_quit);
    assert_eq!(loaded.theme, fragmenta_core::settings::ThemeMode::Dark);
}

#[test]
fn autostart_roundtrip() {
    // autostart = true 保存后重读保留
    let dir = common::TempDir::new("settings-autostart");
    let path = dir.path().join("settings.json");
    let settings = Settings {
        autostart: true,
        ..Default::default()
    };
    settings.save(&path).expect("保存失败");
    let loaded = Settings::load(&path).expect("读取失败");
    assert!(loaded.autostart);
}

#[test]
fn missing_autostart_defaults_false() {
    // 既有 settings.json（无 autostart 字段）容错读取，默认 false
    let dir = common::TempDir::new("settings-autostart-legacy");
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"theme":"Dark","font_scale":"Large","data_dir":"C:/x","tile_hotkey":"ctrl+alt+n","close_to_quit":true}"#,
    )
    .expect("写入失败");
    let loaded = Settings::load(&path).expect("读取失败");
    assert!(!loaded.autostart);
    assert!(loaded.close_to_quit);
}
