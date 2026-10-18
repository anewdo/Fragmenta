//! 笔记功能：共享状态、页面与专用编辑器。

mod editor;
mod page;
mod state;

pub(crate) use page::NotesPage;
pub(crate) use state::NotesStore;
pub use state::{NoteDraft, NoteKey, NotesEvent, NotesState, SaveStatus};
