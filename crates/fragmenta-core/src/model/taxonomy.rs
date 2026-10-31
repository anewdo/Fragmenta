//! 标签实体条目（ADR-0008）：管理页列表与引用计数的载体。

/// 一个标签条目：`color` 为 `#RRGGBB`，`None` 为未选色（中性渲染）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxonomyEntry {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    /// 挂载该标签的笔记数。
    pub note_count: i64,
}
