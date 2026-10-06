//! 领域模型：纯数据类型与派生逻辑，零副作用（架构 §5.1）。

mod filter;
mod note;
mod todo;

pub use filter::{DayActivity, NoteFilter, SearchScope};
pub use note::Note;
pub use todo::Todo;

use std::collections::HashSet;

/// 标签规范化：去除每项首尾空白、丢弃空项、去重（保留首次出现顺序）。
pub fn normalize_tags<I>(tags: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_string();
        if tag.is_empty() {
            continue;
        }
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
    }
    out
}
