//! taxonomy state：标签条目查询与管理页写操作的 read-through 归属
//!
//! 写操作落库后广播 [`TaxonomyEvent::Changed`]，订阅方（笔记页的列表与
//! 筛选选项）各自重拉快照；反向链路（笔记侧写操作刷新管理页）由页面
//! 订阅 `NotesEvent` 完成。

use std::sync::Arc;

use fragmenta_core::db::{Db, Result as DbResult};
use fragmenta_core::model::TaxonomyEntry;

use gpui_kit::{Context, EventEmitter};

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

    pub(crate) fn tags(&self) -> DbResult<Vec<TaxonomyEntry>> {
        self.db.tags_with_usage()
    }

    /// 确保标签存在（不存在时按给定颜色建立，已存在保留原颜色）；返回是否
    /// 新建，新建后广播 `Changed` 供管理页与筛选选项刷新。
    pub(crate) fn ensure_tag(
        &mut self,
        name: &str,
        color: Option<&str>,
        cx: &mut Context<Self>,
    ) -> DbResult<bool> {
        let inserted = self.db.ensure_tag(name, color)?;
        if inserted {
            cx.emit(TaxonomyEvent::Changed);
        }
        Ok(inserted)
    }

    /// 重命名标签（挂载与颜色随实体保留）；成功后广播 `Changed`。
    pub(crate) fn rename(&mut self, from: &str, to: &str, cx: &mut Context<Self>) -> DbResult<()> {
        let result = self.db.rename_tag(from, to);
        if result.is_ok() {
            cx.emit(TaxonomyEvent::Changed);
        }
        result
    }

    /// 设置标签颜色：`Some("#RRGGBB")` 设色、`None` 恢复中性；成功后广播
    /// `Changed`（管理页列表与笔记卡片徽片各自重拉颜色）。
    pub(crate) fn set_color(
        &mut self,
        name: &str,
        color: Option<&str>,
        cx: &mut Context<Self>,
    ) -> DbResult<()> {
        let result = self.db.set_tag_color(name, color);
        if result.is_ok() {
            cx.emit(TaxonomyEvent::Changed);
        }
        result
    }

    /// 删除标签（挂载由数据层摘除）；成功后广播 `Changed`。
    pub(crate) fn delete(&mut self, name: &str, cx: &mut Context<Self>) -> DbResult<()> {
        let result = self.db.delete_tag(name);
        if result.is_ok() {
            cx.emit(TaxonomyEvent::Changed);
        }
        result
    }
}

impl EventEmitter<TaxonomyEvent> for TaxonomyState {}
