//! 笔记页
//! 页面 Entity 常驻于 shell，切换页面时筛选与滚动状态天然保留。

mod filter_panel;
mod taxonomy_dialog;
mod view;

use fragmenta_core::model::{Note, NoteFilter, NoteSort, TaxonomyEntry};

use gpui_kit::component::ResizableState;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use std::collections::HashMap;

use crate::notes::NotesStore;
use crate::notes::editor::NoteEditor;
use crate::notes::{NoteKey, NotesEvent, NotesState};
use crate::taxonomy::{TaxonomyEvent, TaxonomyStore};
use crate::ui::dialog_motion;
use crate::utils::palette::parse_hex_color;
use crate::widgets::toast;

use filter_panel::FilterPanel;
use taxonomy_dialog::TaxonomyModal;

/// 笔记页视图：筛选栏 + 卡片列表，常驻于主窗口 shell。
pub(crate) struct NotesPage {
    notes: Entity<NotesState>,
    /// 搜索框状态（关键词真实值在 InputState，筛选时读取）。
    query_input: Entity<InputState>,
    /// 列表排序方式。
    sort: NoteSort,
    /// 分类筛选（多选，笔记归属任一选中分类即命中）。
    selected_categories: Vec<String>,
    /// 标签筛选（多选，笔记需同时挂有全部所选标签）。
    selected_tags: Vec<String>,
    /// 筛选 Popover 是否打开（受控：关闭时丢弃草稿）。
    filter_open: bool,
    /// 筛选 Popover 内容面板（独立实体：页面重绘不重建筛选树）。
    filter_panel: Entity<FilterPanel>,
    /// 列表快照：查询失败时存错误信息（错误占位 + 重试）。
    list: Result<Vec<Note>, String>,
    /// 名字 → 展示颜色快照（DB 持久颜色，未选色为 None）：卡片徽片与
    /// 标签编辑弹窗按名取色（ADR-0008）。
    tag_colors: HashMap<String, Option<Hsla>>,
    /// 全部标签条目快照（名称排序）：标签编辑弹窗的可选项来源。
    all_tags: Vec<TaxonomyEntry>,
    /// 鼠标当前悬停的卡片（None = 无）；用于操作按钮淡入。
    hovered_note: Option<i64>,
    /// 悬停序号：每进入一张卡片递增，驱动淡入动画重启。
    hover_seq: usize,
    /// 分类与标签弹窗会话（None = 未打开 / 已关闭）。
    taxonomy_modal: Option<TaxonomyModal>,
    /// 弹窗动画序号：每次打开递增，保证淡入淡出动画使用全新元素 id。
    taxonomy_seq: usize,
    /// 右栏编辑器（Phase 10）。
    editor: Entity<NoteEditor>,
    /// 编辑器正打开的笔记 id（None = 空态占位）。
    selected: Option<i64>,
    split_state: Entity<ResizableState>,
    _query_sub: Subscription,
    _changed_sub: Subscription,
    /// 管理页建 / 改 / 删实体 → 列表与筛选选项重拉
    _taxonomy_sub: Subscription,
}

impl NotesPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let notes = cx.global::<NotesStore>().0.clone();
        let query_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索标题或内容…"));
        let page = cx.entity().downgrade();
        let filter_panel = cx.new(|_| FilterPanel::new(page));

        // 关键词每变一字即重拉（本地 SQLite 亚毫秒查询，D5 read-through）；
        // 聚焦 / 失焦只影响搜索框描边，重绘即可
        let query_sub = cx.subscribe(
            &query_input,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.refresh(cx),
                InputEvent::Focus | InputEvent::Blur => cx.notify(),
                _ => {}
            },
        );
        // 订阅矩阵（§5.6）：列表页响应 Changed 重拉列表与筛选选项
        let changed_sub = cx.subscribe(&notes, |this, _, event: &NotesEvent, cx| {
            if matches!(event, NotesEvent::Changed) {
                this.refresh(cx);
            }
        });
        // 管理页建 / 改 / 删实体 → 列表与筛选选项重拉
        let taxonomy = cx.global::<TaxonomyStore>().0.clone();
        let taxonomy_sub = cx.subscribe(&taxonomy, |this, _, _: &TaxonomyEvent, cx| {
            this.refresh(cx);
        });

        let mut this = Self {
            notes,
            query_input,
            sort: NoteSort::default(),
            selected_categories: Vec::new(),
            selected_tags: Vec::new(),
            filter_open: false,
            filter_panel,
            list: Ok(Vec::new()),
            tag_colors: HashMap::new(),
            all_tags: Vec::new(),
            hovered_note: None,
            hover_seq: 0,
            taxonomy_modal: None,
            taxonomy_seq: 0,
            editor: cx.new(|cx| NoteEditor::new(window, cx)),
            selected: None,
            split_state: cx.new(|_| ResizableState::default()),
            _query_sub: query_sub,
            _changed_sub: changed_sub,
            _taxonomy_sub: taxonomy_sub,
        };
        this.refresh(cx);
        this
    }

    /// 由当前筛选状态组装 `NoteFilter` 并重拉全部快照（render 外调用）。
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let filter = NoteFilter {
            query: self.query_input.read(cx).value().to_string(),
            categories: self.selected_categories.clone(),
            tags: self.selected_tags.clone(),
            sort: self.sort,
        };
        let store = self.notes.read(cx);
        self.list = store.notes(&filter).map_err(|err| err.to_string());
        let note_count = store.note_count().unwrap_or(0);
        // 选项查询失败按空处理：列表错误已单独占位呈现
        let taxonomy = cx.global::<TaxonomyStore>().0.clone();
        let taxonomy = taxonomy.read(cx);
        let categories = taxonomy.categories().unwrap_or_default();
        let tags = taxonomy.tags().unwrap_or_default();
        self.tag_colors = tags
            .iter()
            .map(|entry| {
                (
                    entry.name.clone(),
                    entry.color.as_deref().and_then(parse_hex_color),
                )
            })
            .collect();
        self.all_tags = tags.clone();
        self.filter_panel.update(cx, |panel, cx| {
            panel.sync_options(categories, tags, note_count);
            cx.notify();
        });
        // 列表重拉后悬停卡片可能已消失，淡入状态随之复位
        if self.hovered_note.is_some_and(|id| {
            !self
                .list
                .as_ref()
                .is_ok_and(|notes| notes.iter().any(|note| note.id == id))
        }) {
            self.hovered_note = None;
        }
        cx.notify();
    }

    /// 是否存在任一生效的筛选条件（空态文案区分"还没有笔记"与"没有匹配"）。
    fn has_active_filter(&self, cx: &App) -> bool {
        !self.query_input.read(cx).value().is_empty()
            || !self.selected_categories.is_empty()
            || !self.selected_tags.is_empty()
    }

    /// 筛选 Popover 开 / 关时把草稿重置为已生效条件（关闭即丢弃未确认选择）。
    fn reset_filter_draft(&mut self, cx: &mut Context<Self>) {
        self.filter_panel.update(cx, |panel, cx| {
            panel.reset_draft(&self.selected_categories, &self.selected_tags);
            cx.notify();
        });
        cx.notify();
    }

    /// 确认筛选：草稿写回生效条件并重拉列表。
    fn apply_filter_draft(
        &mut self,
        categories: Vec<String>,
        tags: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.selected_categories = categories;
        self.selected_tags = tags;
        self.filter_open = false;
        self.refresh(cx);
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
            self.load_into_editor(note, window, cx);
        }
    }

    /// 把笔记载入右栏编辑器（选中态 + 内容），不含落库与重拉。
    fn load_into_editor(&mut self, note: Note, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(note.id);
        self.editor
            .update(cx, |editor, cx| editor.load(&note, window, cx));
    }

    /// 新增笔记：库里插入空笔记并选中载入编辑器。
    ///
    /// 空内容必然不命中当前搜索 / 筛选条件，因此先清空条件；`create_note`
    /// 的 `Changed` 会同步触发订阅重拉，新卡片随列表快照返回，直接载入
    /// 即可。新笔记没有待写槽，无需 `flush_now`，也不必逐段 `refresh`。
    fn create_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_categories.clear();
        self.selected_tags.clear();
        self.filter_panel.update(cx, |panel, cx| {
            panel.reset_draft(&[], &[]);
            cx.notify();
        });
        self.query_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        match self.notes.update(cx, |state, cx| state.create_note(cx)) {
            Ok(id) => {
                let note = self
                    .list
                    .as_ref()
                    .ok()
                    .and_then(|notes| notes.iter().find(|note| note.id == id).cloned());
                if let Some(note) = note {
                    self.load_into_editor(note, window, cx);
                }
            }
            Err(err) => {
                // 插入失败时不会有 `Changed`：清空后的条件仍需立即生效
                self.refresh(cx);
                toast::error(err.to_string(), window, cx);
            }
        }
    }

    /// 删除确认：Danger 主按钮，确认后走 `NotesState::delete`（Changed 驱动重拉）。
    fn confirm_delete(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let weak = weak.clone();
            dialog_motion::alert_dialog(alert.confirm())
                .title("删除笔记")
                .description("删除后无法恢复，确定要删除这条笔记吗？")
                .ok_text("删除")
                .ok_variant(ButtonVariant::Danger)
                .centered()
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    weak.update(cx, |this, cx| {
                        if let Err(err) = this.notes.update(cx, |s, cx| s.delete(id, cx)) {
                            toast::error(err.to_string(), window, cx);
                            return false;
                        }
                        // 删除协调（§5.6）：正打开的笔记被删 → 同步清空编辑器
                        if this.selected == Some(id) {
                            this.selected = None;
                            this.editor
                                .update(cx, |editor, cx| editor.clear(window, cx));
                        }
                        true
                    })
                    .unwrap_or(true)
                })
        });
    }
}
