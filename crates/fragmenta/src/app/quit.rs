//! 应用退出：保存检查、失败提示与退出决策。

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::h_flex;
use gpui_kit::prelude::*;
use gpui_kit::{App, Global, Window};

use crate::notes::{NotesStore, SaveStatus};
use crate::windows;

/// 保存失败的退出提示存活期间，阻止主窗口关闭与重复弹窗。
struct QuitPrompt;

impl Global for QuitPrompt {}

/// 是否正在等待用户处理退出保存失败。
pub(crate) fn is_pending(cx: &App) -> bool {
    cx.has_global::<QuitPrompt>()
}

/// 统一退出路径：保存成功后退出，失败时保留内容并交由用户决定。
pub(crate) fn request_quit(cx: &mut App) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::ImageStore;
    use crate::notes::NotesState;
    use crate::notes::{NoteDraft, NoteKey};
    use crate::settings::{SettingsState, SettingsStore};
    use crate::taxonomy::{TaxonomyState, TaxonomyStore};
    use crate::todos::{TodosState, TodosStore};
    use crate::widgets::toast::{ToastState, ToastStore};
    use fragmenta_core::db::Db;
    use fragmenta_core::media::Images;
    use fragmenta_core::model::NoteFilter;
    use fragmenta_core::settings::Settings;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{Context, IntoElement, Render, TestAppContext, WindowOptions, div};
    use std::path::PathBuf;
    use std::sync::Arc;

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
        let todos = cx.new(|_| TodosState::new(db.clone()));
        let taxonomy = cx.new(|_| TaxonomyState::new(db));
        let toast = cx.new(|_| ToastState::new());
        let settings = Settings {
            close_to_quit: true,
            ..Settings::default()
        };
        let images = Images::new(settings.data_dir.join("imgs"));
        let settings = cx.new(|cx| {
            SettingsState::new(
                settings,
                PathBuf::from("temp/quit-test-settings.json"),
                Arc::new(Db::open_in_memory().unwrap()),
                cx,
            )
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
            cx.set_global(TaxonomyStore(taxonomy));
            cx.set_global(SettingsStore(settings));
            cx.set_global(ImageStore(images));
            cx.set_global(ToastStore(toast));
            assert!(cx.windows().is_empty());
            request_quit(cx);
        });
        let windows = cx.windows();
        assert_eq!(windows.len(), 1);
        let handle = windows[0];
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("dialog").visible());
        })
        .unwrap();
        cx.update(request_quit);
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
