//! `NotesState`（架构 §5.6 / D4 / D5）：笔记防抖保存、落库与事件广播。

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::time::Duration;

use fragmenta_core::db::{Db, Result as DbResult};
use fragmenta_core::model::{Note, NoteFilter};

use gpui_kit::{Context, EventEmitter, Task};

use super::{Event, NoteDraft, NoteKey, SaveStatus};

/// 防抖窗口：输入停笔后落库的等待时长。
const DEBOUNCE: Duration = Duration::from_millis(300);

/// 一个防抖槽的待写内容与计时任务（ADR-0001：按 NoteKey 分区）。
struct PendingWrite {
    draft: NoteDraft,
    /// 300ms 到期落库的任务；`Task` drop 即取消，重置计时 = 替换任务。
    timer: Option<Task<()>>,
}

/// 笔记 store：磁贴与主界面编辑器共用的写路径（D4）。
///
/// - `save` 内部防抖，窗口内多次提交合并为最后一次内容；
/// - 落库成功发 `Saved` + `Changed`，失败发 `Saved { Failed }` 且不打断后续重试；
/// - 读路径 `notes` 直查，页面在 `Changed` 回调里重拉。
pub struct NotesState {
    db: Arc<Db>,
    pending: HashMap<NoteKey, PendingWrite>,
}

impl EventEmitter<Event> for NotesState {}

impl NotesState {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            pending: HashMap::new(),
        }
    }

    /// 防抖保存：按 `NoteKey` 分区计时，300ms 到期落库该槽内容。
    ///
    /// 窗口内多次提交只保留最后一次内容，且只有最后一个计时任务存活
    /// （新任务覆盖旧任务，旧任务随 `Task` drop 取消）。
    pub fn save(&mut self, draft: NoteDraft, cx: &mut Context<Self>) {
        let key = draft.key.clone();
        let write = match self.pending.entry(key.clone()) {
            Entry::Occupied(mut slot) => {
                slot.get_mut().draft = draft;
                slot.into_mut()
            }
            Entry::Vacant(slot) => slot.insert(PendingWrite { draft, timer: None }),
        };
        write.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            // 实体已释放（应用退出）则放弃本次落库
            let _ = this.update(cx, |state, cx| state.flush(&key, cx));
        }));
    }

    /// 立即落库该槽 pending 内容并取消待触发写入（磁贴关闭路径）。
    ///
    /// 槽位不存在时为无操作；此前落库失败的槽保留在此，此调用即重试。
    pub fn flush_now(&mut self, key: NoteKey, cx: &mut Context<Self>) {
        self.flush(&key, cx);
    }

    /// 删除笔记；成功后广播 `Changed`（note_tags 级联与孤儿标签清理由 db 层负责）。
    pub fn delete(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.db.delete_note(id).is_ok() {
            cx.emit(Event::Changed);
        }
    }

    /// 设置笔记分类（唯一、可清除、新分类自动建立）；成功后广播 `Changed`。
    pub fn set_category(&mut self, id: i64, category: Option<&str>, cx: &mut Context<Self>) {
        if self.db.set_note_category(id, category).is_ok() {
            cx.emit(Event::Changed);
        }
    }

    /// 全量替换笔记标签（入参先经 `normalize_tags` 规范化）；成功后广播 `Changed`。
    pub fn set_tags(&mut self, id: i64, tags: Vec<String>, cx: &mut Context<Self>) {
        if self.db.set_note_tags(id, &tags).is_ok() {
            cx.emit(Event::Changed);
        }
    }

    /// read-through 直查（D5：本地 SQLite 亚毫秒查询，无缓存）。
    pub fn notes(&self, filter: &NoteFilter) -> DbResult<Vec<Note>> {
        self.db.notes(filter)
    }

    /// 取出该槽并落库；失败保留槽位内容（无计时器）供后续 save / flush_now 重试。
    fn flush(&mut self, key: &NoteKey, cx: &mut Context<Self>) {
        let Some(write) = self.pending.remove(key) else {
            return;
        };
        let emit_key = write.draft.key.clone();
        let mut note = write.draft.to_note();
        match self.db.upsert_note(&mut note) {
            Ok(()) => {
                cx.emit(Event::Saved {
                    key: emit_key,
                    id: note.id,
                    status: SaveStatus::Saved,
                });
                cx.emit(Event::Changed);
            }
            Err(_) => {
                self.pending.insert(
                    key.clone(),
                    PendingWrite {
                        draft: write.draft,
                        timer: None,
                    },
                );
                cx.emit(Event::Saved {
                    key: emit_key,
                    id: note.id,
                    status: SaveStatus::Failed,
                });
            }
        }
    }
}
