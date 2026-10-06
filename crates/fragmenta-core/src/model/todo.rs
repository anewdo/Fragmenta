//! 待办模型。

use chrono::{DateTime, Local, NaiveDate};

/// 待办：按日期组织的任务项，可标记完成状态（术语见 GLOSSARY.md）。
#[derive(Debug, Clone, PartialEq)]
pub struct Todo {
    pub id: i64,
    pub title: String,
    pub detail: String,
    /// 归属日期（YYYY-MM-DD）。
    pub date: NaiveDate,
    pub done: bool,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
}
