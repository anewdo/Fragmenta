//! 启动编排（架构 §3 / §5.7 / D3 / D6）：assets、主题、共享 store、热键与主窗口。
//!
//! 运行期产物：exe 同级仅 `settings.json`（数据目录定位的唯一线索，D6 / ADR-0004）；
//! 数据库与图片图库平铺于 `Settings.data_dir`（默认 exe 旁 `data/`，内含
//! `fragmenta.db` 与 `imgs/`）。进程常驻（`QuitMode::Explicit`），
//! 显式退出入口在托盘菜单。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use fragmenta_core::db::Db;
use fragmenta_core::media::Images;
use fragmenta_core::settings::Settings;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::h_flex;
use gpui_kit::prelude::*;
use gpui_kit::{App, AppContext, Entity, Global, QuitMode, Window};

use crate::hotkey::HotkeyController;
use crate::state::{NotesState, SaveStatus, SettingsState, TodosState};
use crate::utils::palette::ColorPalette;
use crate::{tray, windows};

/// App 级共享笔记 store（D3）：主窗口与全部磁贴持有同一 handle。
pub(crate) struct NotesStore(pub(crate) Entity<NotesState>);

impl Global for NotesStore {}

/// App 级共享待办 store（D3）：待办页持有；全部操作同步落库。
pub(crate) struct TodosStore(pub(crate) Entity<TodosState>);

impl Global for TodosStore {}

/// App 级设置 store：运行期设置修改的唯一入口（D8 副作用集中）。
pub(crate) struct SettingsStore(pub(crate) Entity<SettingsState>);

impl Global for SettingsStore {}

/// 图片图库（D6 修订：`imgs/` 平铺于 `data_dir` 内，随迁移整体走）。
pub(crate) struct ImageStore(pub(crate) Images);

impl Global for ImageStore {}

/// 名字取色器（note-taxonomy-form）：卡片徽片与分类标签表单共用同一映射，
/// 保证同屏同名同色；进程内稳定，不持久化。
pub(crate) struct PaletteStore(pub(crate) Mutex<ColorPalette>);

impl Global for PaletteStore {}

/// 保存失败的退出提示存活期间，阻止主窗口关闭与重复弹窗。
pub(crate) struct QuitPrompt;

impl Global for QuitPrompt {}

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
            init_stores(&settings, cx);
            match HotkeyController::init(&settings.tile_hotkey, cx) {
                Ok(controller) => cx.set_global(controller),
                Err(err) => eprintln!("[fragmenta] 磁贴热键注册失败: {err}"),
            }
            tray::init(cx);
            windows::main::open(cx);
        });
}

/// 统一退出路径：保存成功后退出，失败时保留内容并交由用户决定。
pub(crate) fn quit_flushing(cx: &mut App) {
    if cx.has_global::<QuitPrompt>() {
        cx.defer(|cx| {
            windows::main::open(cx);
        });
        return;
    }
    let notes = cx.global::<NotesStore>().0.clone();
    if notes.update(cx, |state, cx| state.flush_all(cx)) == SaveStatus::Saved {
        cx.quit();
        return;
    }
    cx.set_global(QuitPrompt);
    // 关窗回调仍持有当前 Window；延迟更新以避开窗口重入。
    cx.defer(|cx| {
        if let Some(handle) = windows::main::open_handle(cx) {
            if let Err(err) = handle.update(cx, |_, window, cx| quit_failed_dialog(window, cx)) {
                cx.remove_global::<QuitPrompt>();
                eprintln!("[fragmenta] 显示退出提示失败: {err:?}");
            }
        } else {
            cx.remove_global::<QuitPrompt>();
        }
    });
}

fn quit_failed_dialog(window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, |dialog, _, _| {
        dialog
            .title("保存失败")
            .overlay_closable(false)
            .close_button(false)
            .child("部分内容未能保存。仍要退出将丢弃未保存的内容；你也可以重试，或返回界面复制内容到其他地方。")
            .on_ok(|_, _, cx| {
                let notes = cx.global::<NotesStore>().0.clone();
                if notes.update(cx, |state, cx| state.flush_all(cx)) == SaveStatus::Saved {
                    cx.quit();
                }
                false
            })
            .on_cancel(|_, _, cx| {
                cx.remove_global::<QuitPrompt>();
                true
            })
            .footer(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("quit-discard")
                            .danger()
                            .label("仍要退出")
                            .on_click(|_, _, cx| cx.quit()),
                    )
                    .child(Button::new("quit-return").label("返回").on_click(
                        |_, window, cx| {
                            cx.remove_global::<QuitPrompt>();
                            window.close_dialog(cx);
                        },
                    ))
                    .child(
                        Button::new("quit-retry")
                            .primary()
                            .label("重试")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(
                                    Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                                    cx,
                                );
                            }),
                    ),
            )
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
    let db = Arc::new(db);
    let notes = cx.new(|_| NotesState::new(db.clone()));
    let todos = cx.new(|_| TodosState::new(db));
    cx.set_global(NotesStore(notes));
    cx.set_global(TodosStore(todos));
    cx.set_global(ImageStore(Images::new(settings.data_dir.join("imgs"))));
    cx.set_global(PaletteStore(Mutex::new(ColorPalette::new())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{NoteDraft, NoteKey};
    use fragmenta_core::model::NoteFilter;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{Context, IntoElement, Render, TestAppContext, WindowOptions, div};

    struct PromptHost;

    impl Render for PromptHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child("编辑内容")
        }
    }

    #[gpui_kit::test]
    fn failed_retry_keeps_dialog_and_return_keeps_pending(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let db = Arc::new(Db::open_in_memory().unwrap());
        let notes = cx.new(|_| NotesState::new(db.clone()));
        notes.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key: NoteKey::Note(1),
                    title: "待保存".into(),
                    content: "返回后可继续复制".into(),
                },
                cx,
            );
            assert_eq!(state.flush_all(cx), SaveStatus::Failed);
        });
        let (handle, host) = cx.update(|cx| {
            cx.set_global(NotesStore(notes.clone()));
            cx.set_global(QuitPrompt);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| PromptHost))
                .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            quit_failed_dialog(window, cx);
            window.render_frame(cx);
            assert!(window.find("dialog").visible());
            assert!(window.find("quit-discard").visible());
            window.click("quit-retry", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("dialog").visible());
            assert!(cx.has_global::<QuitPrompt>());
            window.click("quit-return", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("dialog").is_none());
            assert!(!cx.has_global::<QuitPrompt>());
        })
        .unwrap();
        let root = handle.downcast::<gpui_kit::base::Root>().unwrap();
        assert_eq!(
            root.read_with(cx, |root, _| root.view().entity_id())
                .unwrap(),
            host.entity_id()
        );
        // 返回后失败内容仍可再次保存。
        let now = chrono::Local::now();
        let mut note = fragmenta_core::model::Note {
            id: 0,
            title: "旧标题".into(),
            content: "旧内容".into(),
            category: None,
            tags: vec![],
            created_at: now,
            updated_at: now,
        };
        db.upsert_note(&mut note).unwrap();
        assert_eq!(
            notes.update(cx, |state, cx| state.flush_all(cx)),
            SaveStatus::Saved
        );
        assert_eq!(
            db.notes(&NoteFilter::default()).unwrap()[0].content,
            "返回后可继续复制"
        );
    }

    #[gpui_kit::test]
    fn quit_failure_opens_main_and_blocks_close_without_stacking_dialogs(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let db = Arc::new(Db::open_in_memory().unwrap());
        let notes = cx.new(|_| NotesState::new(db.clone()));
        let todos = cx.new(|_| TodosState::new(db));
        let settings = Settings {
            close_to_quit: true,
            ..Settings::default()
        };
        let images = Images::new(settings.data_dir.join("imgs"));
        let settings = cx.new(|cx| {
            SettingsState::new(settings, PathBuf::from("temp/quit-test-settings.json"), cx)
        });
        notes.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key: NoteKey::Note(1),
                    title: "待保存".into(),
                    content: "保留内容".into(),
                },
                cx,
            )
        });
        cx.update(|cx| {
            cx.set_global(NotesStore(notes));
            cx.set_global(TodosStore(todos));
            cx.set_global(SettingsStore(settings));
            cx.set_global(ImageStore(images));
            cx.set_global(PaletteStore(Mutex::new(ColorPalette::new())));
            assert!(cx.windows().is_empty());
            quit_flushing(cx);
        });
        let windows = cx.windows();
        assert_eq!(windows.len(), 1);
        let handle = windows[0];
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("dialog").visible());
        })
        .unwrap();
        cx.update(quit_flushing);
        let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
        assert!(!visual.simulate_close());
        cx.simulate_keystrokes(handle, "escape");
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("dialog").is_none());
            assert!(!cx.has_global::<QuitPrompt>());
        })
        .unwrap();
        assert_eq!(cx.windows(), windows);
    }
}
