//! 笔记页（Phase 9）：搜索筛选栏 + 笔记卡片列表；编辑器双栏组装在 Phase 10。
//!
//! 数据流遵循架构铁律：render 内不查询——列表与筛选选项均为回调 /
//! 订阅（`InputEvent::Change` / `Event::Changed`）时拉取的页字段快照；
//! 页面 Entity 常驻于 shell，切换页面时筛选与滚动状态天然保留。
//!
//! 本文件承载状态与交互逻辑（订阅 / 快照拉取 / 对话框），
//! 视图构建（筛选栏 / 卡片 / 空态 / render）见 `view.rs`。

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
use crate::state::{Event, NotesState};

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
    /// 筛选选项快照（分类 / 标签下拉与右键菜单的数据源）。
    categories: Vec<String>,
    tags: Vec<String>,
    /// 重命名对话框 key 序号：每次打开生成全新 InputState，免受上次编辑值残留。
    rename_seq: usize,
    _query_sub: Subscription,
    _changed_sub: Subscription,
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
        cx.notify();
    }

    /// 是否存在任一生效的筛选条件（空态文案区分"还没有笔记"与"没有匹配"）。
    fn has_active_filter(&self, cx: &App) -> bool {
        !self.query_input.read(cx).value().is_empty()
            || self.category.is_some()
            || !self.selected_tags.is_empty()
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
                .on_ok(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.notes.update(cx, |s, cx| s.delete(id, cx));
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
}
