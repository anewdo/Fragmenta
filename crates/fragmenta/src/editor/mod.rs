//! 编辑器组件（架构 §5.9）。

mod image_path;
mod note_editor;
mod source;

pub use image_path::ImagePathPlugin;
pub use note_editor::{EditorMode, NoteEditor};
pub use source::SourceText;
