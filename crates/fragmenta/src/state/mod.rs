//! state 层（架构 §5.6 / D3）：App 级共享 Entity，主窗口与全部磁贴持有同一 handle。
//!
//! 防抖计时、落库、事件广播收在 store 内部（D4）；读路径 read-through 无缓存（D5）。

pub mod notes;

pub use notes::NotesState;

use fragmenta_core::model::Note;
use gpui_kit::EntityId;

/// state 层广播事件：所有订阅方（页面 / 磁贴 / 编辑器）经 `Context::subscribe` 接收。
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// 增删改已落库 → 列表 / 日历重拉（订阅矩阵：磁贴与编辑器不响应此事件）。
    Changed,
    /// 落库结果 → 持有该 key 的磁贴更新保存状态与笔记 id。
    Saved {
        key: NoteKey,
        id: i64,
        status: SaveStatus,
    },
}

/// 落库结果：`Failed` 时磁贴持续提示，下一次防抖保存自然重试（架构 §5.6 错误呈现）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStatus {
    Saved,
    Failed,
}

/// 防抖槽身份（ADR-0001）：未落库草稿用调用方稳定标识，落库后用笔记 id。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NoteKey {
    Draft(EntityId),
    Note(i64),
}

/// 待保存草稿：携带防抖槽身份与完整标题 / 正文（每次提交均为全量内容）。
#[derive(Debug, Clone)]
pub struct NoteDraft {
    pub key: NoteKey,
    pub title: String,
    pub content: String,
}

impl NoteDraft {
    /// 转为待写入的 `Note`：`Draft` 无 id（插入路径），`Note(id)` 走更新路径；
    /// 时间戳为占位值，以 `upsert_note` 回填的库中数据为准。
    fn to_note(&self) -> Note {
        let now = chrono::Local::now();
        Note {
            id: match &self.key {
                NoteKey::Note(id) => *id,
                NoteKey::Draft(_) => 0,
            },
            title: self.title.clone(),
            content: self.content.clone(),
            category: None,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}
