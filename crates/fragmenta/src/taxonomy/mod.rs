//! 标签管理页。

mod page;
mod state;
mod tag_modal;

use gpui_kit::{Entity, Global};

pub(crate) use page::TaxonomyPage;
pub(crate) use state::{TaxonomyEvent, TaxonomyState};

/// 功能入口的状态登记（主窗口页面经 global 取用）。
pub(crate) struct TaxonomyStore(pub Entity<TaxonomyState>);

impl Global for TaxonomyStore {}
