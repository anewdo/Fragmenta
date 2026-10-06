//! 筛选条件与聚合结果类型。

use chrono::NaiveDate;

/// 笔记搜索范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope {
    /// 仅匹配标题。
    Title,
    /// 全文：标题与正文都参与匹配。
    #[default]
    FullText,
}

/// 笔记列表筛选条件；各维度为"与"的关系，标签之间同为"与"。
#[derive(Debug, Clone, Default)]
pub struct NoteFilter {
    /// 关键词（空串 = 不筛选）。
    pub query: String,
    pub scope: SearchScope,
    /// 分类（None = 不筛选）。
    pub category: Option<String>,
    /// 标签（空 = 不筛选；笔记需同时挂有全部所选标签）。
    pub tags: Vec<String>,
}

/// 首页日历聚合：某日的笔记与待办数量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayActivity {
    pub date: NaiveDate,
    pub note_count: i64,
    pub todo_count: i64,
}
