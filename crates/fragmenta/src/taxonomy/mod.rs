//! 分类 / 标签管理页。
//!

mod page;
mod state;

use fragmenta_core::model::TaxonomyEntry;

use gpui_kit::assets::IconName;
use gpui_kit::{Entity, Global};

pub(crate) use page::TaxonomyPage;
pub(crate) use state::{TaxonomyEvent, TaxonomyState};

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
    /// 实体名（界面文案用）。
    pub(crate) fn label(self) -> &'static str {
        match self {
            TaxonomyKind::Category => "分类",
            TaxonomyKind::Tag => "标签",
        }
    }

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
            TaxonomyKind::Category => "在上方输入行输入分类名，回车即可新建",
            TaxonomyKind::Tag => "在上方输入行输入标签名，回车即可新建",
        }
    }

    /// 空态图标。
    pub(crate) fn icon(self) -> IconName {
        match self {
            TaxonomyKind::Category => IconName::Folder,
            TaxonomyKind::Tag => IconName::Tags,
        }
    }

    /// 新建输入行占位文案。
    pub(crate) fn create_placeholder(self) -> &'static str {
        match self {
            TaxonomyKind::Category => "新建分类，回车保存",
            TaxonomyKind::Tag => "新建标签，回车保存",
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
