//! 启动编排（架构 §3 / §5.7 / D3 / D6）：assets、主题、共享 store、热键与主窗口。
//!
//! 运行期产物：exe 同级仅 `settings.json`（数据目录定位的唯一线索，D6 / ADR-0004）；
//! 数据库与图片图库平铺于 `Settings.data_dir`（默认 exe 旁 `data/`，内含
//! `fragmenta.db` 与 `imgs/`）。进程常驻（`QuitMode::Explicit`），
//! 显式退出入口在托盘菜单。

pub(crate) mod quit;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use fragmenta_core::db::{Db, Result as DbResult};
use fragmenta_core::media::Images;
use fragmenta_core::settings::Settings;
use gpui_kit::{App, AppContext, QuitMode};

use crate::editor::ImageStore;
use crate::hotkey::HotkeyController;
use crate::notes::NotesState;
use crate::notes::NotesStore;
use crate::settings::SettingsState;
use crate::settings::SettingsStore;
use crate::todos::TodosState;
use crate::todos::TodosStore;
use crate::utils::palette::ColorPalette;
use crate::utils::palette::PaletteStore;
use crate::{tray, windows};

/// exe 所在目录；`settings.json` 的锚点（数据目录线索唯一入口）。
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 应用入口；`main.rs` 仅调用此函数。
pub fn run() {
    let settings_path = exe_dir().join("settings.json");
    let settings = Settings::load(&settings_path).unwrap_or_else(|err| {
        eprintln!("[fragmenta] 读取设置失败，使用默认设置: {err}");
        Settings::default()
    });
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets::new(""))
        .run(move |cx| {
            gpui_kit::init(cx);
            // 进程常驻（spike ⑤ 结论）：末窗关闭不退出，退出走统一退出路径
            cx.set_quit_mode(QuitMode::Explicit);
            // 设置 store 构造即应用主题与字号（D8 副作用集中，无启动期双轨）
            let settings_state =
                cx.new(|cx| SettingsState::new(settings.clone(), settings_path.clone(), cx));
            cx.set_global(SettingsStore(settings_state));
            if let Err(err) = init_stores(&settings, cx) {
                eprintln!(
                    "[fragmenta] 数据库打开失败（{}）: {err}",
                    settings.data_dir.join("fragmenta.db").display()
                );
                std::process::exit(1);
            }
            match HotkeyController::init(&settings.tile_hotkey, cx) {
                Ok(controller) => cx.set_global(controller),
                Err(err) => eprintln!("[fragmenta] 磁贴热键注册失败: {err}"),
            }
            tray::init(cx);
            windows::main::open(cx);
        });
}

/// 建立共享 store，数据库初始化失败时由启动入口决定终止。
fn init_stores(settings: &Settings, cx: &mut App) -> DbResult<()> {
    let db_path = settings.data_dir.join("fragmenta.db");
    let db = Arc::new(Db::open(&db_path)?);
    let notes = cx.new(|_| NotesState::new(db.clone()));
    let todos = cx.new(|_| TodosState::new(db));
    cx.set_global(NotesStore(notes));
    cx.set_global(TodosStore(todos));
    cx.set_global(ImageStore(Images::new(settings.data_dir.join("imgs"))));
    cx.set_global(PaletteStore(Mutex::new(ColorPalette::new())));
    Ok(())
}
