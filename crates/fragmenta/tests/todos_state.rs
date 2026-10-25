//! TodosState headless 测试：断言库中数据与事件序列（真实 SQLite，无 mock 无桩）。

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use chrono::{NaiveDate, NaiveTime};
use fragmenta::todos::{TodosEvent, TodosState};
use fragmenta_core::db::Db;
use gpui_kit::{AppContext as _, Entity, TestAppContext};

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn time(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}

/// 订阅 store 事件并收集为序列。
fn track_events(state: &Entity<TodosState>, cx: &TestAppContext) -> Rc<RefCell<Vec<TodosEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|cx| {
        cx.subscribe(state, move |_, event: &TodosEvent, _| {
            sink.borrow_mut().push(*event);
        })
        .detach();
    });
    events
}

/// add 落库（内容 / 日期 / 区间 / 回填 id）并广播一次 Changed。
#[gpui_kit::test]
fn add_persists_and_broadcasts(cx: &mut TestAppContext) {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let state = cx.new(|_| TodosState::new(db));
    let events = track_events(&state, cx);

    let added = state.update(cx, |state, cx| {
        state
            .add(
                "写周报",
                date(2026, 10, 11),
                Some((time(9, 0), time(10, 0))),
                cx,
            )
            .unwrap()
    });
    assert!(added.id > 0);

    let todos = state.read_with(cx, |state, _| {
        state
            .todos_between(date(2026, 10, 11), date(2026, 10, 11))
            .unwrap()
    });
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "写周报");
    assert_eq!(todos[0].interval, Some((time(9, 0), time(10, 0))));
    assert!(!todos[0].done);
    assert_eq!(*events.borrow(), vec![TodosEvent::Changed]);
}

/// toggle 翻转完成态：每落库一次广播一次 Changed。
#[gpui_kit::test]
fn toggle_flips_done(cx: &mut TestAppContext) {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let state = cx.new(|_| TodosState::new(db));
    let events = track_events(&state, cx);

    let added = state.update(cx, |state, cx| {
        state.add("买咖啡", date(2026, 10, 11), None, cx).unwrap()
    });
    state.update(cx, |state, cx| state.toggle(&added, cx).unwrap());
    let todos = state.read_with(cx, |state, _| {
        state
            .todos_between(date(2026, 10, 11), date(2026, 10, 11))
            .unwrap()
    });
    assert!(todos[0].done);

    state.update(cx, |state, cx| state.toggle(&todos[0], cx).unwrap());
    let todos = state.read_with(cx, |state, _| {
        state
            .todos_between(date(2026, 10, 11), date(2026, 10, 11))
            .unwrap()
    });
    assert!(!todos[0].done);
    assert_eq!(*events.borrow(), vec![TodosEvent::Changed; 3]);
}

/// update 修改内容与区间（含清除区间变回全天）并广播 Changed。
#[gpui_kit::test]
fn update_changes_content_and_interval(cx: &mut TestAppContext) {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let state = cx.new(|_| TodosState::new(db));
    let events = track_events(&state, cx);

    let mut todo = state.update(cx, |state, cx| {
        state
            .add(
                "上午会",
                date(2026, 10, 11),
                Some((time(9, 0), time(10, 0))),
                cx,
            )
            .unwrap()
    });
    todo.content = "下午会".into();
    todo.interval = None;
    state.update(cx, |state, cx| state.update(&mut todo, cx).unwrap());

    let todos = state.read_with(cx, |state, _| {
        state
            .todos_between(date(2026, 10, 11), date(2026, 10, 11))
            .unwrap()
    });
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "下午会");
    assert_eq!(todos[0].interval, None);
    assert_eq!(*events.borrow(), vec![TodosEvent::Changed; 2]);
}

/// delete 落库并广播 Changed；删除后清单为空。
#[gpui_kit::test]
fn delete_removes_and_broadcasts(cx: &mut TestAppContext) {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let state = cx.new(|_| TodosState::new(db));
    let events = track_events(&state, cx);

    let added = state.update(cx, |state, cx| {
        state.add("临时事项", date(2026, 10, 11), None, cx).unwrap()
    });
    state.update(cx, |state, cx| state.delete(added.id, cx));

    let todos = state.read_with(cx, |state, _| {
        state
            .todos_between(date(2026, 10, 11), date(2026, 10, 11))
            .unwrap()
    });
    assert!(todos.is_empty());
    assert_eq!(*events.borrow(), vec![TodosEvent::Changed; 2]);
}

/// todos_between 闭区间边界与排序（全天置顶 → 开始时间 → id）。
#[gpui_kit::test]
fn todos_between_bounds_and_ordering(cx: &mut TestAppContext) {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let state = cx.new(|_| TodosState::new(db));

    let day = date(2026, 10, 11);
    state.update(cx, |state, cx| {
        state
            .add("下午两点", day, Some((time(14, 0), time(15, 0))), cx)
            .unwrap();
        state.add("全天", day, None, cx).unwrap();
        state
            .add("早九", day, Some((time(9, 0), time(9, 30))), cx)
            .unwrap();
        // 边界日期：闭区间端点包含在内
        state.add("月末", date(2026, 10, 31), None, cx).unwrap();
    });

    let todos = state.read_with(cx, |state, _| {
        state
            .todos_between(date(2026, 10, 11), date(2026, 10, 31))
            .unwrap()
    });
    let contents: Vec<&str> = todos.iter().map(|t| t.content.as_str()).collect();
    assert_eq!(contents, vec!["全天", "早九", "下午两点", "月末"]);

    let none = state.read_with(cx, |state, _| {
        state
            .todos_between(date(2026, 10, 12), date(2026, 10, 30))
            .unwrap()
    });
    assert!(none.is_empty());
}
