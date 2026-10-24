//! 分类 / 标签管理页（taxonomy-pages spec，ADR-0008）。
//!
//! 分类页与标签页由同一通用组件装配：列表行依次为色点（未选色为中性
//! 空点）、名称、引用数，名称字典序排序。状态层为 read-through 的登记
//! 类型 TaxonomyStore（架构 §5.7 D3），页面禁止直接 import 数据层。

mod page;
mod state;

use fragmenta_core::model::TaxonomyEntry;

use gpui_kit::assets::IconName;
use gpui_kit::{Entity, Global};

pub(crate) use page::TaxonomyPage;
pub(crate) use state::TaxonomyState;

/// 功能入口的状态登记（主窗口页面经 global 取用）。
pub(crate) struct TaxonomyStore(pub Entity<TaxonomyState>);

impl Global for TaxonomyStore {}

/// 实体种类：装配同一通用管理组件的两条枚举分支。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TaxonomyKind {
    Category,
    Tag,
}

impl TaxonomyKind {
    /// 空态标题。
    pub(crate) fn empty_title(self) -> &'static str {
        match self {
            TaxonomyKind::Category => "还没有分类",
            TaxonomyKind::Tag => "还没有标签",
        }
    }

    /// 空态说明。
    pub(crate) fn empty_description(self) -> &'static str {
        match self {
            TaxonomyKind::Category => "在笔记卡片的「分类与标签」表单里输入即可添加",
            TaxonomyKind::Tag => "在笔记卡片的「分类与标签」表单里输入即可添加",
        }
    }

    /// 空态图标。
    pub(crate) fn icon(self) -> IconName {
        match self {
            TaxonomyKind::Category => IconName::Folder,
            TaxonomyKind::Tag => IconName::Tags,
        }
    }

    /// 本种类的条目列表（read-through：分类 / 标签各取其表）。
    pub(crate) fn entries(
        self,
        store: &TaxonomyState,
    ) -> Result<Vec<TaxonomyEntry>, fragmenta_core::db::DbError> {
        match self {
            TaxonomyKind::Category => store.categories(),
            TaxonomyKind::Tag => store.tags(),
        }
    }
}
