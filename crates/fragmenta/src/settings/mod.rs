//! 设置功能：运行期状态、设置页与内置色系。

mod page;
mod state;
mod theme_schemes;

pub(crate) use page::SettingsPage;
pub(crate) use state::SettingsStore;
pub use state::{SettingsEvent, SettingsState};
