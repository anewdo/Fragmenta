//! `TodosState`（架构 §5.6 / D4 / D5）：待办同步落库与事件广播（无防抖需求）。

use std::sync::Arc;

use chrono::{Local, NaiveDate, NaiveTime};
use fragmenta_core::db::{Db, Result as DbResult};
use fragmenta_core::model::Todo;

use gpui_kit::{Context, Entity, EventEmitter, Global};

/// App 级共享待办 store（D3）：待办页持有；全部操作同步落库。
pub(crate) struct TodosStore(pub(crate) Entity<TodosState>);

impl Global for TodosStore {}

/// 待办 store 事件：增删改落库成功后广播，页面订阅后重拉当日清单。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodosEvent {
    Changed,
}

/// 待办 store：全部操作同步落库，读路径 read-through 无缓存（D5）。
pub struct TodosState {
    db: Arc<Db>,
}

impl EventEmitter<TodosEvent> for TodosState {}

impl TodosState {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// 新建待办；落库成功回填 id 与时间戳并广播 `Changed`。
    pub fn add(
        &mut self,
        content: &str,
        date: NaiveDate,
        interval: Option<(NaiveTime, NaiveTime)>,
        cx: &mut Context<Self>,
    ) -> DbResult<Todo> {
        let now = Local::now();
        let mut todo = Todo {
            id: 0,
            content: content.to_string(),
            date,
            interval,
            done: false,
            created_at: now,
            updated_at: now,
        };
        match self.db.upsert_todo(&mut todo) {
            Ok(()) => {
                cx.emit(TodosEvent::Changed);
                Ok(todo)
            }
            Err(err) => Err(err),
        }
    }

    /// 翻转完成态；成功广播 `Changed`。
    pub fn toggle(&mut self, todo: &Todo, cx: &mut Context<Self>) -> DbResult<()> {
        let mut updated = todo.clone();
        updated.done = !updated.done;
        let result = self.db.upsert_todo(&mut updated);
        if result.is_ok() {
            cx.emit(TodosEvent::Changed);
        }
        result
    }

    /// 更新待办（内容 / 日期 / 区间 / 完成态以传入值为准，时间戳以库中回填为准）；
    /// 成功广播 `Changed`。
    pub fn update(&mut self, todo: &mut Todo, cx: &mut Context<Self>) -> DbResult<()> {
        let result = self.db.upsert_todo(todo);
        if result.is_ok() {
            cx.emit(TodosEvent::Changed);
        }
        result
    }

    /// 删除待办；成功广播 `Changed`（目标不存在时 db 层为无操作，不广播）。
    pub fn delete(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.db.delete_todo(id).is_ok() {
            cx.emit(TodosEvent::Changed);
        }
    }

    /// read-through 直查闭区间 [from, to]（D5：本地 SQLite 亚毫秒查询，无缓存）。
    pub fn todos_between(&self, from: NaiveDate, to: NaiveDate) -> DbResult<Vec<Todo>> {
        self.db.todos_between(from, to)
    }
}
