//! 筛选条件与聚合结果类型。

use chrono::NaiveDate;

/// 笔记列表排序方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteSort {
    /// 最近修改优先。
    #[default]
    UpdatedAt,
    /// 最近创建优先。
    CreatedAt,
    /// 按标题名称升序。
    Title,
}

/// 笔记列表筛选条件；各维度为"与"的关系，标签之间同为"与"。
#[derive(Debug, Clone, Default)]
pub struct NoteFilter {
    /// 关键词（空串 = 不筛选）：标题与正文都参与匹配。
    pub query: String,
    /// 分类（空 = 不筛选；笔记归属任一选中的分类即命中）。
    pub categories: Vec<String>,
    /// 标签（空 = 不筛选；笔记需同时挂有全部所选标签）。
    pub tags: Vec<String>,
    /// 列表排序方式。
    pub sort: NoteSort,
}

/// 首页日历聚合：某日的笔记与待办数量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayActivity {
    pub date: NaiveDate,
    pub note_count: i64,
    pub todo_count: i64,
}
