//! 待办功能：共享状态与月历页面。

mod page;
mod state;

pub(crate) use page::TodosPage;
pub(crate) use state::TodosStore;
pub use state::{TodosEvent, TodosState};
