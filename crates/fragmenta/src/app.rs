//! 启动编排（架构 §3 / §5.7 / D3 / D6）：assets、主题、共享 store、热键与主窗口。
//!
//! 运行期产物位于 exe 同级：`settings.json`、`imgs/`；数据库在
//! `Settings.data_dir`（默认 exe 旁 `data/`）下。进程常驻（`QuitMode::Explicit`），
//! 显式退出入口在设置页。

use std::path::PathBuf;
use std::sync::Arc;

use fragmenta_core::db::Db;
use fragmenta_core::media::Images;
use fragmenta_core::settings::{FontScale, Settings, ThemeMode};
use gpui_kit::component::{Theme, ThemeMode as KitThemeMode};
use gpui_kit::{App, AppContext, Entity, Global, QuitMode, WindowAppearance, px};

use crate::hotkey::HotkeyController;
use crate::state::NotesState;
use crate::windows;

/// App 级共享笔记 store（D3）：主窗口与全部磁贴持有同一 handle。
pub(crate) struct NotesStore(pub(crate) Entity<NotesState>);

impl Global for NotesStore {}

/// 图片图库（D6：`imgs/` 固定在 exe 旁，与 `data_dir` 无关）。
pub(crate) struct ImageStore(pub(crate) Images);

impl Global for ImageStore {}

/// exe 所在目录；`settings.json` / `imgs/` 的锚点。
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 应用入口；`main.rs` 仅调用此函数。
pub fn run() {
    let settings = Settings::load(&exe_dir().join("settings.json")).unwrap_or_else(|err| {
        eprintln!("[fragmenta] 读取设置失败，使用默认设置: {err}");
        Settings::default()
    });
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets::new(""))
        .run(move |cx| {
            gpui_kit::init(cx);
            apply_theme(&settings, cx);
            // 进程常驻（spike ⑤ 结论）：末窗关闭不退出，显式退出走设置页
            cx.set_quit_mode(QuitMode::Explicit);
            init_stores(&settings, cx);
            match HotkeyController::init(&settings.tile_hotkey, cx) {
                Ok(controller) => cx.set_global(controller),
                Err(err) => eprintln!("[fragmenta] 磁贴热键注册失败: {err}"),
            }
            windows::main::open(cx);
        });
}

/// 建立 App 级共享 store：数据库连接 + 图库。
///
/// 数据库打开失败属致命故障（磁盘异常 / 路径不可写），直接退出并说明原因；
/// 磁贴与页面的可重试失败在各自写路径上呈现（§5.6 错误呈现）。
fn init_stores(settings: &Settings, cx: &mut App) {
    let db_path = settings.data_dir.join("fragmenta.db");
    let db = match Db::open(&db_path) {
        Ok(db) => db,
        Err(err) => {
            eprintln!("[fragmenta] 数据库打开失败（{}）: {err}", db_path.display());
            std::process::exit(1);
        }
    };
    let notes = cx.new(|_| NotesState::new(Arc::new(db)));
    cx.set_global(NotesStore(notes));
    cx.set_global(ImageStore(Images::new(exe_dir().join("imgs"))));
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
