//! 磁贴 headless 测试：保存闭环、笔记列表、今日待办与内联新增表单。

use super::{SaveIndicator, TileMode, TileView};
use crate::editor::ImageStore;
use crate::notes::NotesStore;
use crate::notes::{NoteDraft, NoteKey, NotesState, SaveStatus};
use crate::settings::SettingsState;
use crate::settings::SettingsStore;
use crate::todos::{TodosState, TodosStore};
use crate::widgets::toast::{ToastState, ToastStore};
use chrono::Local;
use fragmenta_core::{
    db::Db,
    media::Images,
    model::{Note, NoteFilter, Todo},
    settings::Settings,
};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext,
    WindowOptions,
};
use std::{path::PathBuf, sync::Arc};

fn open_tile(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<TileView>, Arc<Db>) {
    cx.update(gpui_kit::init);
    let db = Arc::new(Db::open_in_memory().unwrap());
    let notes = cx.new(|_| NotesState::new(db.clone()));
    let todos = cx.new(|_| TodosState::new(db.clone()));
    let settings = cx.new(|cx| {
        SettingsState::new(
            Settings::default(),
            PathBuf::from("temp/tile-test-settings.json"),
            db.clone(),
            cx,
        )
    });
    let (handle, tile) = cx.update(|cx| {
        cx.set_global(NotesStore(notes));
        cx.set_global(TodosStore(todos));
        cx.set_global(SettingsStore(settings));
        let toast = cx.new(|_| ToastState::new());
        cx.set_global(ToastStore(toast));
        cx.set_global(ImageStore(Images::new(PathBuf::from(
            "temp/tile-test-imgs",
        ))));
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| TileView::new(window, cx))
        })
        .unwrap()
    });
    (handle, tile, db)
}

fn insert_note(db: &Db, title: &str, content: &str) -> Note {
    let now = Local::now();
    let mut note = Note {
        id: 0,
        title: title.into(),
        content: content.into(),
        tags: Vec::new(),
        created_at: now,
        updated_at: now,
    };
    db.upsert_note(&mut note).unwrap();
    note
}

fn insert_todo(db: &Db, content: &str, date: chrono::NaiveDate) -> Todo {
    let now = Local::now();
    let mut todo = Todo {
        id: 0,
        content: content.into(),
        date,
        interval: None,
        done: false,
        created_at: now,
        updated_at: now,
    };
    db.upsert_todo(&mut todo).unwrap();
    todo
}

#[gpui_kit::test]
fn native_close_saves_nonempty_tile(cx: &mut TestAppContext) {
    let (handle, tile, db) = open_tile(cx);
    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            tile.title.update(cx, |input, cx| {
                input.set_value("原生关闭前保存", window, cx)
            });
            tile.dispatch_save(cx);
        });
    })
    .unwrap();
    let mut visual = VisualTestContext::from_window(handle, cx);
    assert!(visual.simulate_close());
    let notes = db.notes(&NoteFilter::default()).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "原生关闭前保存");
}

#[gpui_kit::test]
fn native_close_failure_keeps_window_and_content(cx: &mut TestAppContext) {
    let (handle, tile, _) = open_tile(cx);
    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            tile.key = NoteKey::Note(1);
            tile.title.update(cx, |input, cx| {
                input.set_value("保存失败仍保留", window, cx)
            });
            tile.dispatch_save(cx);
        });
    })
    .unwrap();
    let mut visual = VisualTestContext::from_window(handle, cx);
    assert!(!visual.simulate_close());
    assert!(cx.windows().contains(&handle));
    tile.read_with(cx, |tile, cx| {
        assert!(tile.status == SaveIndicator::Failed);
        assert_eq!(tile.title.read(cx).value().as_str(), "保存失败仍保留");
    });
}

#[gpui_kit::test]
fn native_close_empty_tile_cancels_pending(cx: &mut TestAppContext) {
    let (handle, tile, db) = open_tile(cx);
    tile.update(cx, |tile, cx| {
        let key = tile.key.clone();
        tile.notes.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key,
                    title: "过期待写内容".into(),
                    content: String::new(),
                },
                cx,
            )
        });
    });
    let mut visual = VisualTestContext::from_window(handle, cx);
    assert!(visual.simulate_close());
    let notes = cx.update(|cx| cx.global::<NotesStore>().0.clone());
    assert_eq!(
        notes.update(cx, |state, cx| state.flush_all(cx)),
        SaveStatus::Saved
    );
    assert!(db.notes(&NoteFilter::default()).unwrap().is_empty());
}

#[gpui_kit::test]
fn notes_search_covers_title_and_content_and_survives_mode_switch(cx: &mut TestAppContext) {
    let (handle, tile, db) = open_tile(cx);
    insert_note(&db, "命中标题", "正文");
    insert_note(&db, "其他", "正文命中");
    insert_note(&db, "无关", "无关");
    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            tile.toggle_mode(TileMode::Notes, window, cx);
            assert!(
                tile.query_input
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            );
            tile.query_input.update(cx, |input, cx| {
                input.set_value("命中", window, cx);
            });
            tile.refresh_notes(cx);
            assert_eq!(tile.note_list.as_ref().unwrap().len(), 2);

            tile.toggle_mode(TileMode::Notes, window, cx);
            assert_eq!(tile.mode, TileMode::Editor);
            assert!(
                tile.source
                    .textarea()
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            );
            tile.toggle_mode(TileMode::Notes, window, cx);
            assert_eq!(tile.query_input.read(cx).value().as_str(), "命中");
            assert_eq!(tile.note_list.as_ref().unwrap().len(), 2);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn open_note_flushes_pending_writes_and_loads_latest_content(cx: &mut TestAppContext) {
    let (handle, tile, db) = open_tile(cx);
    let target = insert_note(&db, "目标旧标题", "目标旧正文");
    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            tile.title.update(cx, |input, cx| {
                input.set_value("当前草稿", window, cx);
            });
            tile.source.set_value("当前正文", window, cx);
            tile.dispatch_save(cx);
            tile.notes.update(cx, |state, cx| {
                state.save(
                    NoteDraft {
                        key: NoteKey::Note(target.id),
                        title: "目标新标题".into(),
                        content: "目标新正文".into(),
                    },
                    cx,
                );
            });
            tile.toggle_mode(TileMode::Notes, window, cx);
            tile.open_note(target.id, window, cx);

            assert_eq!(tile.mode, TileMode::Editor);
            assert_eq!(tile.key, NoteKey::Note(target.id));
            assert_eq!(tile.title.read(cx).value().as_str(), "目标新标题");
            assert_eq!(tile.source.value(cx), "目标新正文");
        });
    })
    .unwrap();
    let notes = db.notes(&NoteFilter::default()).unwrap();
    assert!(
        notes
            .iter()
            .any(|note| note.title == "当前草稿" && note.content == "当前正文")
    );
    let target = notes.iter().find(|note| note.id == target.id).unwrap();
    assert_eq!(target.title, "目标新标题");
    assert_eq!(target.content, "目标新正文");
}

#[gpui_kit::test]
fn today_todos_toggle_done_and_cancel_midnight_task_on_exit(cx: &mut TestAppContext) {
    let (handle, tile, db) = open_tile(cx);
    let today = Local::now().date_naive();
    let todo = insert_todo(&db, "今日事项", today);
    insert_todo(&db, "昨日事项", today.pred_opt().unwrap());
    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            tile.toggle_mode(TileMode::Todos, window, cx);
            assert_eq!(tile.todo_date, today);
            assert_eq!(tile.todo_list.as_ref().unwrap().len(), 1);
            assert!(tile.midnight_task.is_some());
            tile.toggle_todo(todo.clone(), window, cx);
        });
    })
    .unwrap();
    cx.executor().run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            assert!(tile.todo_list.as_ref().unwrap()[0].done);
            let done = tile.todo_list.as_ref().unwrap()[0].clone();
            tile.toggle_todo(done, window, cx);
        });
    })
    .unwrap();
    cx.executor().run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            assert!(!tile.todo_list.as_ref().unwrap()[0].done);
            tile.toggle_mode(TileMode::Todos, window, cx);
            assert_eq!(tile.mode, TileMode::Editor);
            assert!(tile.midnight_task.is_none());
        });
    })
    .unwrap();
    assert!(!db.todos_between(today, today).unwrap()[0].done);
}

#[gpui_kit::test]
fn todo_form_collapsible_keeps_state_across_mode_switches(cx: &mut TestAppContext) {
    let (handle, tile, db) = open_tile(cx);
    let today = Local::now().date_naive();
    insert_todo(&db, "收起表单后可见的待办", today);
    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            tile.toggle_mode(TileMode::Todos, window, cx);
            // 收起后清单仍完整呈现
            tile.toggle_todo_form(window, cx);
            assert!(!tile.todo_form_open);
            assert_eq!(tile.todo_list.as_ref().unwrap().len(), 1);
            // 展开态跨模式切换保留：切走再切回仍收起，且不再抢焦点
            tile.toggle_mode(TileMode::Todos, window, cx);
            tile.toggle_mode(TileMode::Todos, window, cx);
            assert_eq!(tile.mode, TileMode::Todos);
            assert!(!tile.todo_form_open);
            assert!(!tile.todo_form.read(cx).content_focus(cx).is_focused(window));
            // 展开：焦点落内容输入框，输入后照常提交
            tile.toggle_todo_form(window, cx);
            assert!(tile.todo_form_open);
            assert!(tile.todo_form.read(cx).content_focus(cx).is_focused(window));
            tile.todo_form.update(cx, |form, cx| {
                form.set_content("收起再展开后的待办", window, cx);
                cx.notify();
            });
            tile.submit_todo(window, cx);
        });
    })
    .unwrap();
    cx.executor().run_until_parked();
    let todos = db.todos_between(today, today).unwrap();
    assert_eq!(todos.len(), 2);
    assert!(
        todos
            .iter()
            .any(|todo| todo.content == "收起再展开后的待办")
    );
}

#[gpui_kit::test]
fn todo_form_focuses_adds_resets_and_delete_removes(cx: &mut TestAppContext) {
    let (handle, tile, db) = open_tile(cx);
    let today = Local::now().date_naive();
    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            tile.toggle_mode(TileMode::Todos, window, cx);
            // 切入 Todos 模式：焦点落新增表单的内容输入框
            assert!(tile.todo_form.read(cx).content_focus(cx).is_focused(window));
            // 空内容提交不落库（按钮禁用的双保险路径）
            tile.submit_todo(window, cx);
        });
    })
    .unwrap();
    assert!(db.todos_between(today, today).unwrap().is_empty());

    cx.update_window(handle, |_, window, cx| {
        tile.update(cx, |tile, cx| {
            tile.todo_form.update(cx, |form, cx| {
                form.set_content("磁贴新待办", window, cx);
                cx.notify();
            });
            tile.submit_todo(window, cx);
            // 提交成功后表单复位（连续录入）
            assert!(tile.todo_form.read(cx).content_empty(cx));
        });
    })
    .unwrap();
    cx.executor().run_until_parked();

    // 落库成功 + Changed 事件驱动磁贴清单刷新
    let todos = db.todos_between(today, today).unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "磁贴新待办");
    assert_eq!(todos[0].date, today);
    assert_eq!(todos[0].interval, None);
    tile.read_with(cx, |tile, _| {
        assert_eq!(tile.todo_list.as_ref().unwrap().len(), 1);
    });

    // 行内 hover「删除」按钮路径：直接落库删除
    tile.update(cx, |tile, cx| tile.delete_todo(todos[0].id, cx));
    cx.executor().run_until_parked();
    assert!(db.todos_between(today, today).unwrap().is_empty());
    tile.read_with(cx, |tile, _| {
        assert!(tile.todo_list.as_ref().unwrap().is_empty());
    });
}
