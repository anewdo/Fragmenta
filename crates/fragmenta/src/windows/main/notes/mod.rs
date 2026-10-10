//! 笔记页（Phase 9 / 10）：搜索筛选栏 + 笔记卡片列表 + 右栏 Markdown 编辑器。
//!
//! 数据流遵循架构铁律：render 内不查询——列表与筛选选项均为回调 /
//! 订阅（`InputEvent::Change` / `Event::Changed`）时拉取的页字段快照；
//! 页面 Entity 常驻于 shell，切换页面时筛选与滚动状态天然保留。
//!
//! 本文件承载状态与交互逻辑（订阅 / 快照拉取 / 选中载入 / 对话框 /
//! 分类标签表单会话），视图构建（筛选栏 / 卡片 / 空态 / 双栏 render）
//! 见 `view.rs`，分类与标签编辑表单（Popover 内容）见 `taxonomy_form.rs`。

mod taxonomy_form;
mod view;

use fragmenta_core::model::{Note, NoteFilter, SearchScope};

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::dialog::DialogContent;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::NotesStore;
use crate::editor::{EditorMode, NoteEditor};
use crate::state::{Event, NoteKey, NotesState};

/// 笔记页视图：筛选栏 + 卡片列表，常驻于主窗口 shell。
pub struct NotesPage {
    notes: Entity<NotesState>,
    /// 搜索框状态（关键词真实值在 InputState，筛选时读取）。
    query_input: Entity<InputState>,
    /// 关键词匹配范围。
    scope: SearchScope,
    /// 分类筛选（None = 不限）。
    category: Option<String>,
    /// 标签筛选（多选，笔记需同时挂有全部所选标签）。
    selected_tags: Vec<String>,
    /// 列表快照：查询失败时存错误信息（错误占位 + 重试）。
    list: Result<Vec<Note>, String>,
    /// 筛选选项快照（分类 / 标签下拉的数据源）。
    categories: Vec<String>,
    tags: Vec<String>,
    /// 当前打开的分类与标签编辑表单会话（同一时刻至多一个）。
    taxonomy_form: Option<TaxonomyForm>,
    /// 右栏编辑器（Phase 10）。
    editor: Entity<NoteEditor>,
    /// 编辑器正打开的笔记 id（None = 空态占位）。
    selected: Option<i64>,
    /// 重命名对话框 key 序号：每次打开生成全新 InputState，免受上次编辑值残留。
    rename_seq: usize,
    _query_sub: Subscription,
    _changed_sub: Subscription,
}

/// 分类与标签编辑表单会话（note-taxonomy-form spec）。
///
/// 随 Popover 打开而创建、关闭而整体释放：两个输入框为本次打开的全新
/// 实体（不同卡片 / 多次打开互不残留），回车订阅随会话 drop 自动退订。
pub struct TaxonomyForm {
    note_id: i64,
    category_input: Entity<InputState>,
    tag_input: Entity<InputState>,
    _category_sub: Subscription,
    _tag_sub: Subscription,
}

impl NotesPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let notes = cx.global::<NotesStore>().0.clone();
        let query_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索笔记…"));

        // 关键词每变一字即重拉（本地 SQLite 亚毫秒查询，D5 read-through）
        let query_sub = cx.subscribe(&query_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refresh(cx);
            }
        });
        // 订阅矩阵（§5.6）：列表页响应 Changed 重拉列表与筛选选项
        let changed_sub = cx.subscribe(&notes, |this, _, event: &Event, cx| {
            if matches!(event, Event::Changed) {
                this.refresh(cx);
            }
        });

        let mut this = Self {
            notes,
            query_input,
            scope: SearchScope::default(),
            category: None,
            selected_tags: Vec::new(),
            list: Ok(Vec::new()),
            categories: Vec::new(),
            tags: Vec::new(),
            taxonomy_form: None,
            editor: cx.new(|cx| NoteEditor::new(window, cx)),
            selected: None,
            rename_seq: 0,
            _query_sub: query_sub,
            _changed_sub: changed_sub,
        };
        this.refresh(cx);
        this
    }

    /// 由当前筛选状态组装 `NoteFilter` 并重拉全部快照（render 外调用）。
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let filter = NoteFilter {
            query: self.query_input.read(cx).value().to_string(),
            scope: self.scope,
            category: self.category.clone(),
            tags: self.selected_tags.clone(),
        };
        let store = self.notes.read(cx);
        self.list = store.notes(&filter).map_err(|err| err.to_string());
        // 选项查询失败按空处理：列表错误已单独占位呈现
        self.categories = store.categories().unwrap_or_default();
        self.tags = store.tags().unwrap_or_default();
        // 表单所属笔记已不在列表（如打开期间被删除）时结束会话，
        // 释放输入实体与回车订阅；卡片随列表消失，Popover 无处挂载。
        if let Some(form) = self.taxonomy_form.as_ref() {
            let id = form.note_id;
            let visible = self
                .list
                .as_ref()
                .is_ok_and(|notes| notes.iter().any(|note| note.id == id));
            if !visible {
                self.taxonomy_form = None;
            }
        }
        cx.notify();
    }

    /// 是否存在任一生效的筛选条件（空态文案区分"还没有笔记"与"没有匹配"）。
    fn has_active_filter(&self, cx: &App) -> bool {
        !self.query_input.read(cx).value().is_empty()
            || self.category.is_some()
            || !self.selected_tags.is_empty()
    }

    /// 编辑器当前模式（模式切换按钮组由主窗口标题栏承载）。
    pub fn editor_mode(&self, cx: &App) -> EditorMode {
        self.editor.read(cx).mode()
    }

    /// 切换编辑器模式。
    pub fn set_editor_mode(&mut self, mode: EditorMode, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| editor.set_mode(mode, cx));
    }

    /// 选中卡片 → 编辑器载入（Phase 10）。
    ///
    /// 载入前先 `flush_now` 目标 key 并重拉快照：目标笔记的待写内容可能
    /// 来自磁贴（或本编辑器上次编辑后 300ms 内被切走），先落库再读，
    /// 保证编辑器载入的是最新内容；前一笔记的 pending 防抖仍由 state
    /// 保障落库（ADR-0001 按 key 分区，互不干扰）。重复点击同一卡片为
    /// 无操作（编辑中点击自身卡片不清空、不重载）。
    fn select_note(&mut self, note_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected == Some(note_id) {
            return;
        }
        self.notes.update(cx, |state, cx| {
            state.flush_now(NoteKey::Note(note_id), cx);
        });
        self.refresh(cx);
        let note = self
            .list
            .as_ref()
            .ok()
            .and_then(|notes| notes.iter().find(|note| note.id == note_id).cloned());
        if let Some(note) = note {
            self.selected = Some(note_id);
            self.editor
                .update(cx, |editor, cx| editor.load(&note, window, cx));
        }
    }

    /// 删除确认：Danger 主按钮，确认后走 `NotesState::delete`（Changed 驱动重拉）。
    fn confirm_delete(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let weak = weak.clone();
            alert
                .confirm()
                .title("删除笔记")
                .description("删除后无法恢复，确定要删除这条笔记吗？")
                .ok_text("删除")
                .ok_variant(ButtonVariant::Danger)
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.notes.update(cx, |s, cx| s.delete(id, cx));
                        // 删除协调（§5.6）：正打开的笔记被删 → 同步清空编辑器
                        //（cancel 该 key 待写槽，免于过期 upsert 虚警）
                        if this.selected == Some(id) {
                            this.selected = None;
                            this.editor
                                .update(cx, |editor, cx| editor.clear(window, cx));
                        }
                    });
                    true
                })
        });
    }

    /// 分类重命名对话框：输入新名走 `rename_category`，失败以通知呈现错误。
    ///
    /// InputState 经 `use_keyed_state` 在 content 闭包外创建一次（content
    /// 每帧调用，闭包内不可建实体）；序号掺入 key 使每次打开都重置输入。
    fn open_rename_category(&mut self, from: String, window: &mut Window, cx: &mut Context<Self>) {
        self.rename_seq += 1;
        let seq = self.rename_seq;
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            let initial = from.clone();
            let input = window.use_keyed_state(
                format!("rename-category-{seq}-{from}"),
                cx,
                move |window, cx| {
                    let mut state = InputState::new(window, cx).placeholder("分类名");
                    state.set_value(initial, window, cx);
                    state
                },
            );
            let input_for_content = input.clone();
            dialog
                .title("重命名分类")
                .width(px(380.))
                .content(move |_, _, _| {
                    DialogContent::new().child(Input::new(&input_for_content).w_full())
                })
                .on_ok({
                    let weak = weak.clone();
                    let input = input.clone();
                    let from = from.clone();
                    move |_, window, cx| {
                        let to = input.read(cx).value().to_string();
                        if to.is_empty() || to == from {
                            return true;
                        }
                        let result = weak.update(cx, |this, cx| {
                            this.notes
                                .update(cx, |s, cx| s.rename_category(&from, &to, cx))
                        });
                        if let Ok(Err(err)) = result {
                            window.push_notification(Notification::error(err.to_string()), cx);
                        }
                        true
                    }
                })
        });
    }

    /// 打开分类与标签表单会话：两个全新输入框 + 回车订阅（经
    /// `Window::subscribe`，回车回调可拿 window 以清空输入框）。
    ///
    /// 逐条即时保存：回车即落库并广播 `Changed`（列表与表单随刷新同步），
    /// 输入框清空以便连续录入；空白回车不产生数据、仅清空。
    fn open_taxonomy_form(&mut self, note_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let category_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("输入分类，回车保存"));
        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("输入标签，回车保存"));

        let weak = cx.entity().downgrade();
        let weak_category = weak.clone();
        let category_sub =
            window.subscribe(&category_input, cx, move |input, event, window, app| {
                if !matches!(event, InputEvent::PressEnter { .. }) {
                    return;
                }
                let value = input.read(app).value().to_string();
                let _ = weak_category.update(app, |this, cx| {
                    this.save_taxonomy_category(note_id, &value, cx)
                });
                input.update(app, |state, cx| state.set_value("", window, cx));
            });

        let weak_tag = weak.clone();
        let tag_sub = window.subscribe(&tag_input, cx, move |input, event, window, app| {
            if !matches!(event, InputEvent::PressEnter { .. }) {
                return;
            }
            let value = input.read(app).value().to_string();
            let _ = weak_tag.update(app, |this, cx| this.add_taxonomy_tag(note_id, &value, cx));
            input.update(app, |state, cx| state.set_value("", window, cx));
        });

        // 覆盖旧会话（若残存）：旧订阅随 drop 退订，旧输入实体随之释放
        self.taxonomy_form = Some(TaxonomyForm {
            note_id,
            category_input,
            tag_input,
            _category_sub: category_sub,
            _tag_sub: tag_sub,
        });
        cx.notify();
    }

    /// 关闭表单会话：订阅随 drop 退订，输入实体释放，卡片回到纯按钮形态。
    fn close_taxonomy_form(&mut self, note_id: i64, cx: &mut Context<Self>) {
        if self
            .taxonomy_form
            .as_ref()
            .is_some_and(|form| form.note_id == note_id)
        {
            self.taxonomy_form = None;
            cx.notify();
        }
    }

    /// 分类回车保存：单值语义，非空输入替换旧分类。
    fn save_taxonomy_category(&mut self, note_id: i64, value: &str, cx: &mut Context<Self>) {
        let value = value.trim();
        if value.is_empty() {
            return;
        }
        self.notes
            .update(cx, |s, cx| s.set_category(note_id, Some(value), cx));
    }

    /// 标签回车保存：多值语义，追加到现有标签之后（重复时不落库，输入框仍清空）。
    fn add_taxonomy_tag(&mut self, note_id: i64, value: &str, cx: &mut Context<Self>) {
        let value = value.trim();
        if value.is_empty() {
            return;
        }
        let Some(mut tags) = self.note_tags(note_id) else {
            return;
        };
        if tags.iter().any(|tag| tag == value) {
            return;
        }
        tags.push(value.to_string());
        self.notes.update(cx, |s, cx| s.set_tags(note_id, tags, cx));
    }

    /// 点击分类 Tag：清除分类。
    fn remove_taxonomy_category(&mut self, note_id: i64, cx: &mut Context<Self>) {
        self.notes
            .update(cx, |s, cx| s.set_category(note_id, None, cx));
    }

    /// 点击标签 Tag：从现有标签中移除该标签。
    fn remove_taxonomy_tag(&mut self, note_id: i64, tag: &str, cx: &mut Context<Self>) {
        let Some(mut tags) = self.note_tags(note_id) else {
            return;
        };
        if tags.iter().all(|existing| existing != tag) {
            return;
        }
        tags.retain(|existing| existing != tag);
        self.notes.update(cx, |s, cx| s.set_tags(note_id, tags, cx));
    }

    /// 列表快照中该笔记的当前标签（表单打开期间笔记必在列表中）。
    fn note_tags(&self, note_id: i64) -> Option<Vec<String>> {
        self.list
            .as_ref()
            .ok()?
            .iter()
            .find(|note| note.id == note_id)
            .map(|note| note.tags.clone())
    }
}
