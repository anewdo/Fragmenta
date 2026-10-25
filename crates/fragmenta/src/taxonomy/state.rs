//! taxonomy state：条目查询与管理页写操作的 read-through 归属
//!
//! 写操作落库后广播 [`TaxonomyEvent::Changed`]，订阅方（笔记页的列表与
//! 筛选选项）各自重拉快照；反向链路（笔记侧写操作刷新管理页）由页面
//! 订阅 `NotesEvent` 完成。

use std::sync::Arc;

use fragmenta_core::db::{Db, Result as DbResult};
use fragmenta_core::model::TaxonomyEntry;

use gpui_kit::{Context, EventEmitter};

use super::TaxonomyKind;

/// 管理页实体写操作（建 / 改名 / 删除）已落库。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaxonomyEvent {
    /// 实体集合或挂载关系已变化 → 订阅方重拉列表与筛选选项。
    Changed,
}

pub(crate) struct TaxonomyState {
    db: Arc<Db>,
}

impl TaxonomyState {
    pub(crate) fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub(crate) fn categories(&self) -> DbResult<Vec<TaxonomyEntry>> {
        self.db.categories_with_usage()
    }

    pub(crate) fn tags(&self) -> DbResult<Vec<TaxonomyEntry>> {
        self.db.tags_with_usage()
    }

    /// 新建实体（默认无色）；成功后广播 `Changed`。
    pub(crate) fn create(
        &mut self,
        kind: TaxonomyKind,
        name: &str,
        cx: &mut Context<Self>,
    ) -> DbResult<()> {
        let result = match kind {
            TaxonomyKind::Category => self.db.create_category(name),
            TaxonomyKind::Tag => self.db.create_tag(name),
        };
        if result.is_ok() {
            cx.emit(TaxonomyEvent::Changed);
        }
        result
    }

    /// 重命名实体（挂载与颜色随实体保留）；成功后广播 `Changed`。
    pub(crate) fn rename(
        &mut self,
        kind: TaxonomyKind,
        from: &str,
        to: &str,
        cx: &mut Context<Self>,
    ) -> DbResult<()> {
        let result = match kind {
            TaxonomyKind::Category => self.db.rename_category(from, to),
            TaxonomyKind::Tag => self.db.rename_tag(from, to),
        };
        if result.is_ok() {
            cx.emit(TaxonomyEvent::Changed);
        }
        result
    }

    /// 删除实体（分类置空归属 / 标签摘除挂载由数据层完成）；成功后广播 `Changed`。
    pub(crate) fn delete(
        &mut self,
        kind: TaxonomyKind,
        name: &str,
        cx: &mut Context<Self>,
    ) -> DbResult<()> {
        let result = match kind {
            TaxonomyKind::Category => self.db.delete_category(name),
            TaxonomyKind::Tag => self.db.delete_tag(name),
        };
        if result.is_ok() {
            cx.emit(TaxonomyEvent::Changed);
        }
        result
    }
}

impl EventEmitter<TaxonomyEvent> for TaxonomyState {}
