//! 设置功能：运行期状态与设置页。

mod fonts;
mod migrate;
mod page;
mod state;

pub use fonts::FontScope;
pub(crate) use page::SettingsPage;
pub(crate) use state::SettingsStore;
pub use state::{SettingsEvent, SettingsState};
