//! `NotesState`（架构 §5.6 / D4 / D5）：笔记防抖保存、落库与事件广播。

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::time::Duration;

use fragmenta_core::db::{Db, Result as DbResult};
use fragmenta_core::model::{Note, NoteFilter};

use gpui_kit::{Context, Entity, EntityId, EventEmitter, Global, Task};

/// App 级共享笔记 store（D3）：主窗口与全部磁贴持有同一 handle。
pub(crate) struct NotesStore(pub(crate) Entity<NotesState>);

impl Global for NotesStore {}

/// state 层广播事件：所有订阅方（页面 / 磁贴 / 编辑器）经 `Context::subscribe` 接收。
#[derive(Debug, Clone, PartialEq)]
pub enum NotesEvent {
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

impl EventEmitter<NotesEvent> for NotesState {}

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

    /// 立即落库该槽 pending 内容并取消待触发写入，返回落库结果（磁贴关闭路径）。
    ///
    /// 槽位不存在时返回 `None`（无待写内容）；此前落库失败的槽保留在此，
    /// 此调用即重试，调用方依返回值决定是否放行后续动作（如关窗）。
    pub fn flush_now(&mut self, key: NoteKey, cx: &mut Context<Self>) -> Option<SaveStatus> {
        self.flush(&key, cx)
    }

    /// 丢弃该槽待写内容：移除槽位、不落库、不发事件。
    ///
    /// 供磁贴两条路径使用——空磁贴关闭（免于落出空笔记）与草稿晋升后清理
    /// 旧 `Draft` 槽（防计时竞态落出重复内容）。
    pub fn cancel(&mut self, key: &NoteKey) {
        self.pending.remove(key);
    }

    /// 立即尝试落库全部 pending 槽；任一失败则返回 `Failed`，失败内容保留供重试。
    pub fn flush_all(&mut self, cx: &mut Context<Self>) -> SaveStatus {
        let keys: Vec<NoteKey> = self.pending.keys().cloned().collect();
        let mut status = SaveStatus::Saved;
        for key in keys {
            if self.flush(&key, cx) == Some(SaveStatus::Failed) {
                status = SaveStatus::Failed;
            }
        }
        status
    }

    /// 删除成功后取消该笔记的待写槽与计时器，再广播 `Changed`。
    pub fn delete(&mut self, id: i64, cx: &mut Context<Self>) -> DbResult<()> {
        self.db.delete_note(id)?;
        self.cancel(&NoteKey::Note(id));
        cx.emit(NotesEvent::Changed);
        Ok(())
    }

    /// 设置笔记分类（唯一、可清除、新分类自动建立）；成功后广播 `Changed`。
    pub fn set_category(
        &mut self,
        id: i64,
        category: Option<&str>,
        cx: &mut Context<Self>,
    ) -> DbResult<()> {
        self.db.set_note_category(id, category)?;
        cx.emit(NotesEvent::Changed);
        Ok(())
    }

    /// 全量替换笔记标签（入参先经 `normalize_tags` 规范化）；成功后广播 `Changed`。
    pub fn set_tags(&mut self, id: i64, tags: Vec<String>, cx: &mut Context<Self>) -> DbResult<()> {
        self.db.set_note_tags(id, &tags)?;
        cx.emit(NotesEvent::Changed);
        Ok(())
    }

    /// read-through 直查（D5：本地 SQLite 亚毫秒查询，无缓存）。
    pub fn notes(&self, filter: &NoteFilter) -> DbResult<Vec<Note>> {
        self.db.notes(filter)
    }

    /// read-through：全部分类名（按名称排序）。
    pub fn categories(&self) -> DbResult<Vec<String>> {
        self.db.categories()
    }

    /// read-through：全部标签名（按名称排序；标签为显式实体，无引用亦留存）。
    pub fn tags(&self) -> DbResult<Vec<String>> {
        self.db.tags()
    }

    /// read-through：分类名 → 颜色（`#RRGGBB`，未选色为 None），
    /// 供卡片徽片与分类标签表单按名取色。
    pub fn category_colors(&self) -> DbResult<HashMap<String, Option<String>>> {
        self.db.category_colors()
    }

    /// read-through：标签名 → 颜色（`#RRGGBB`，未选色为 None）。
    pub fn tag_colors(&self) -> DbResult<HashMap<String, Option<String>>> {
        self.db.tag_colors()
    }

    /// 重命名分类；成功后广播 `Changed`（列表与筛选选项跟随刷新）。
    ///
    /// 返回结果供调用方呈现错误（源不存在 / 目标重名）。
    pub fn rename_category(
        &mut self,
        from: &str,
        to: &str,
        cx: &mut Context<Self>,
    ) -> DbResult<()> {
        let result = self.db.rename_category(from, to);
        if result.is_ok() {
            cx.emit(NotesEvent::Changed);
        }
        result
    }

    /// 取出该槽并落库，返回结果；失败保留槽位内容（无计时器）供后续 save / flush_now 重试。
    fn flush(&mut self, key: &NoteKey, cx: &mut Context<Self>) -> Option<SaveStatus> {
        let write = self.pending.remove(key)?;
        let emit_key = write.draft.key.clone();
        let mut note = write.draft.to_note();
        match self.db.upsert_note(&mut note) {
            Ok(()) => {
                cx.emit(NotesEvent::Saved {
                    key: emit_key,
                    id: note.id,
                    status: SaveStatus::Saved,
                });
                cx.emit(NotesEvent::Changed);
                Some(SaveStatus::Saved)
            }
            Err(_) => {
                self.pending.insert(
                    key.clone(),
                    PendingWrite {
                        draft: write.draft,
                        timer: None,
                    },
                );
                cx.emit(NotesEvent::Saved {
                    key: emit_key,
                    id: note.id,
                    status: SaveStatus::Failed,
                });
                Some(SaveStatus::Failed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{AppContext as _, TestAppContext};

    #[gpui_kit::test]
    fn delete_cancels_only_the_deleted_notes_pending_write(cx: &mut TestAppContext) {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let state = cx.new(|_| NotesState::new(db.clone()));
        let draft = cx.new(|_| ()).entity_id();
        let mut note = NoteDraft {
            key: NoteKey::Draft(draft),
            title: "删除目标".into(),
            content: "已有内容".into(),
        }
        .to_note();
        db.upsert_note(&mut note).unwrap();
        state.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key: NoteKey::Note(note.id),
                    title: "删除目标".into(),
                    content: "尚未落库的编辑".into(),
                },
                cx,
            );
            state.save(
                NoteDraft {
                    key: NoteKey::Draft(draft),
                    title: "另一篇笔记".into(),
                    content: "应正常保存".into(),
                },
                cx,
            );
            state.delete(note.id, cx).unwrap();
            assert!(!state.pending.contains_key(&NoteKey::Note(note.id)));
            assert!(state.pending.contains_key(&NoteKey::Draft(draft)));
        });
        cx.executor().run_until_parked();
        cx.executor().advance_clock(DEBOUNCE);
        cx.executor().run_until_parked();
        state.update(cx, |state, cx| {
            assert!(state.pending.is_empty());
            assert_eq!(state.flush_all(cx), SaveStatus::Saved);
        });
        let notes = db.notes(&NoteFilter::default()).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].content, "应正常保存");
    }

    #[gpui_kit::test]
    fn delete_clears_a_previously_failed_pending_write(cx: &mut TestAppContext) {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let state = cx.new(|_| NotesState::new(db.clone()));
        state.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key: NoteKey::Note(1),
                    title: "失败槽".into(),
                    content: "等待重试".into(),
                },
                cx,
            );
            assert_eq!(state.flush_all(cx), SaveStatus::Failed);
            state.delete(1, cx).unwrap();
            assert!(state.pending.is_empty());
            assert_eq!(state.flush_all(cx), SaveStatus::Saved);
        });
        assert!(db.notes(&NoteFilter::default()).unwrap().is_empty());
    }

    #[gpui_kit::test]
    fn flush_all_attempts_every_slot_and_retains_failures(cx: &mut TestAppContext) {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let state = cx.new(|_| NotesState::new(db.clone()));
        let draft = cx.new(|_| ()).entity_id();
        state.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key: NoteKey::Note(100),
                    title: "失败内容".into(),
                    content: "保留供重试".into(),
                },
                cx,
            );
            state.save(
                NoteDraft {
                    key: NoteKey::Draft(draft),
                    title: "成功内容".into(),
                    content: "完整落库".into(),
                },
                cx,
            );
            assert_eq!(state.flush_all(cx), SaveStatus::Failed);
            assert_eq!(state.pending.len(), 1);
            assert_eq!(
                state.pending[&NoteKey::Note(100)].draft.content,
                "保留供重试"
            );
            assert!(state.pending[&NoteKey::Note(100)].timer.is_none());
            assert_eq!(state.flush_all(cx), SaveStatus::Failed);
        });
        let notes = db.notes(&NoteFilter::default()).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].content, "完整落库");
    }

    #[gpui_kit::test]
    fn flush_all_retries_retained_content(cx: &mut TestAppContext) {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let state = cx.new(|_| NotesState::new(db.clone()));
        let mut note = NoteDraft {
            key: NoteKey::Draft(cx.new(|_| ()).entity_id()),
            title: "原内容".into(),
            content: "原内容".into(),
        }
        .to_note();
        state.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key: NoteKey::Note(1),
                    title: "待保存标题".into(),
                    content: "待保存内容".into(),
                },
                cx,
            );
            assert_eq!(state.flush_all(cx), SaveStatus::Failed);
        });
        // 补回缺失记录，消除真实 SQLite 更新失败的原因。
        db.upsert_note(&mut note).unwrap();
        assert_eq!(note.id, 1);
        state.update(cx, |state, cx| {
            assert_eq!(state.flush_all(cx), SaveStatus::Saved);
            assert!(state.pending.is_empty());
            assert_eq!(state.flush_all(cx), SaveStatus::Saved);
        });
        let notes = db.notes(&NoteFilter::default()).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "待保存标题");
        assert_eq!(notes[0].content, "待保存内容");
    }
}
