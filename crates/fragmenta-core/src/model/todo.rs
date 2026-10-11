//! 待办模型。

use chrono::{DateTime, Local, NaiveDate, NaiveTime};

/// 待办：按日期组织的任务项，可标记完成状态（术语见 GLOSSARY.md）。
#[derive(Debug, Clone, PartialEq)]
pub struct Todo {
    pub id: i64,
    /// 内容（单字段；v2 迁移前 title/detail 合并而来）。
    pub content: String,
    /// 归属日期（YYYY-MM-DD）。
    pub date: NaiveDate,
    /// 可选时间区间（None = 全天待办；结束晚于开始，不支持跨天）。
    pub interval: Option<(NaiveTime, NaiveTime)>,
    pub done: bool,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
}
