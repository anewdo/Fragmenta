//! 分类 / 标签管理的状态层：read-through 包装（架构 §5.7），全应用 SQL
//! 归 fragmenta-core（架构铁律）。

use std::sync::Arc;

use fragmenta_core::db::{Db, DbError};
use fragmenta_core::model::TaxonomyEntry;

/// 分类 / 标签实体状态：持有共享 `Db`，向两页提供 read-through 查询。
pub(crate) struct TaxonomyState {
    db: Arc<Db>,
}

impl TaxonomyState {
    pub(crate) fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// 全部分类条目（名称排序）：颜色 + 引用笔记数。
    pub(crate) fn categories(&self) -> Result<Vec<TaxonomyEntry>, DbError> {
        self.db.categories_with_usage()
    }

    /// 全部标签条目（名称排序）：颜色 + 挂载笔记数。
    pub(crate) fn tags(&self) -> Result<Vec<TaxonomyEntry>, DbError> {
        self.db.tags_with_usage()
    }
}
