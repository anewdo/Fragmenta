//! 磁贴视图：顶栏操作 + 笔记编辑 / 列表 / 今日待办 + 原生边缘 resize。

use std::time::Duration;

use chrono::{Local, NaiveDate, TimeZone};
use fragmenta_core::model::{Note, NoteFilter, Todo};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Selectable as _, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::editor::ImageStore;
use crate::editor::SourceText;
use crate::notes::NotesStore;
use crate::notes::{NoteDraft, NoteKey, NotesEvent, NotesState, SaveStatus};
use crate::settings::SettingsEvent;
use crate::settings::SettingsStore;
use crate::todos::{TodosEvent, TodosState, TodosStore};
use crate::ui::focus::{TextFieldExt as _, input_border};
use crate::ui::note_list;
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};
use crate::utils::time;
use crate::widgets::toast::{self, ToastState, ToastStore};
use crate::win32;

use super::CORNER;

/// 边缘热区条宽（逻辑像素）：WM_NCHITTEST 子类化的命中带宽。
const EDGE_HOT: f32 = 6.5;
/// 角部热区边长（逻辑像素）：对角 resize 命中区。
const CORNER_HOT: f32 = 14.;
const NOTE_SUMMARY_LINES: usize = 1;
/// 磁贴笔记行标题 / 摘要字号：与笔记页卡片保持一致（固定，不随小窗字号缩放）。
const TILE_NOTE_TITLE_FONT: Pixels = px(13.);
const TILE_NOTE_SUMMARY_FONT: Pixels = px(11.);
const TODO_CHECK_SIZE: Pixels = px(16.);

/// 顶栏保存状态（区别于 state 层 `SaveStatus`：补充 UI 侧"未触碰 / 防抖中"两态）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveIndicator {
    /// 尚无任何输入。
    Untouched,
    /// 有输入待落库（防抖计时段）。
    Pending,
    /// 最近一次落库成功。
    Saved,
    /// 最近一次落库失败；持续显示直至再次成功。
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TileMode {
    Editor,
    Notes,
    Todos,
}

/// 磁贴视图：顶栏操作 + 标题 / 正文输入 + 防抖保存闭环。
pub(super) struct TileView {
    /// 防抖槽身份：未落库草稿为 `Draft`(自身实体 id)，首存成功后切换为 `Note(id)`。
    key: NoteKey,
    notes: Entity<NotesState>,
    todos: Entity<TodosState>,
    title: Entity<InputState>,
    source: SourceText,
    mode: TileMode,
    query_input: Entity<InputState>,
    note_list: Result<Vec<Note>, String>,
    todo_list: Result<Vec<Todo>, String>,
    todo_date: NaiveDate,
    /// toast 浮层渲染载体：App 级共享实体，仅渲染本窗口触发的条目。
    toast: Entity<ToastState>,
    midnight_task: Option<Task<()>>,
    topmost: bool,
    status: SaveIndicator,
    word_count: usize,
    /// 最近一次圆角区域参数 (宽, 高, scale)：未变则跳过重设。
    region: Option<(f32, f32, f32)>,
    _title_sub: Subscription,
    _word_sub: Subscription,
    _query_sub: Subscription,
    _saved_sub: Subscription,
    _notes_changed_sub: Subscription,
    _todos_changed_sub: Subscription,
    _bounds_sub: Subscription,
    /// 小窗字号变化 → 标题 / 正文随 tile_font_px 重渲染。
    _settings_sub: Subscription,
    _appearance_sub: Subscription,
}

impl TileView {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let notes = cx.global::<NotesStore>().0.clone();
        let todos = cx.global::<TodosStore>().0.clone();
        let toast = cx.global::<ToastStore>().0.clone();
        let source = SourceText::new(
            cx.global::<ImageStore>().0.clone(),
            "写点什么……",
            window,
            cx,
        );
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("标题"));
        let query_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索标题或内容…"));

        let title_sub = cx.subscribe(&title, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.dispatch_save(cx);
            }
        });
        // 字数回调兼作正文保存钩子：正文 Change → 更新字数 + 提交防抖
        let word_sub = source.on_word_count(cx, |this, count, cx| {
            this.word_count = count;
            this.dispatch_save(cx);
        });
        let query_sub = cx.subscribe(
            &query_input,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.refresh_notes(cx),
                InputEvent::Focus | InputEvent::Blur => cx.notify(),
                _ => {}
            },
        );
        let saved_sub = cx.subscribe(&notes, |this, _, event: &NotesEvent, cx| {
            if let NotesEvent::Saved { key, id, status } = event
                && key == &this.key
            {
                match status {
                    SaveStatus::Saved => {
                        this.status = SaveIndicator::Saved;
                        if let NoteKey::Draft(_) = &this.key {
                            this.promote(*id, cx);
                        }
                    }
                    SaveStatus::Failed => this.status = SaveIndicator::Failed,
                }
                cx.notify();
            }
        });
        let notes_changed_sub = cx.subscribe(&notes, |this, _, event: &NotesEvent, cx| {
            if matches!(event, NotesEvent::Changed) && this.mode == TileMode::Notes {
                this.refresh_notes(cx);
            }
        });
        let todos_changed_sub = cx.subscribe(&todos, |this, _, event: &TodosEvent, cx| {
            if matches!(event, TodosEvent::Changed) && this.mode == TileMode::Todos {
                this.refresh_todos(cx);
            }
        });

        // 八向边缘 resize：WM_NCHITTEST 子类化补齐框架缺口（隐藏标题栏时
        // 框架只算顶边），命中后由系统原生模态循环接管；最小尺寸经
        // window_min_size 由后端 WM_GETMINMAXINFO 原生钳制
        let _ = win32::install_edge_subclass(window, EDGE_HOT, CORNER_HOT);
        // 圆角窗口区域（失败不阻断：界面层圆角描边兜底）
        let _ = win32::set_rounded_region(window, CORNER);
        // 窗口 bounds 变化（用户 resize / 拖动 / 跨显示器 scale 变化）时重设区域，
        // 并把调整后的尺寸记入设置（下次呼出沿用，防抖写盘）
        let bounds_sub = cx.observe_window_bounds(window, |this, window, cx| {
            this.refresh_rounded_region(window);
            this.record_size(window, cx);
        });

        let settings = cx.global::<SettingsStore>().0.clone();
        let appearance_sub =
            settings.update(cx, |state, cx| state.attach_appearance_bridge(window, cx));
        // 小窗字号变化 → 标题 / 正文重渲染（tile_font_px）
        let settings_sub = cx.subscribe(&settings, |_, _, _: &SettingsEvent, cx| {
            cx.notify();
        });
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            weak.update(cx, |this, cx| this.can_close(cx))
                .unwrap_or(true)
        });

        // 焦点落正文：热键呼出后立即输入
        let focus = source.textarea().read(cx).focus_handle(cx);
        window.focus(&focus, cx);

        Self {
            key: NoteKey::Draft(cx.entity().entity_id()),
            notes,
            todos,
            title,
            source,
            mode: TileMode::Editor,
            query_input,
            note_list: Ok(Vec::new()),
            todo_list: Ok(Vec::new()),
            todo_date: Local::now().date_naive(),
            toast,
            midnight_task: None,
            topmost: true, // PopUp 默认携带 WS_EX_TOPMOST（spike ④ 验证）
            status: SaveIndicator::Untouched,
            word_count: 0,
            region: None,
            _title_sub: title_sub,
            _word_sub: word_sub,
            _query_sub: query_sub,
            _saved_sub: saved_sub,
            _notes_changed_sub: notes_changed_sub,
            _todos_changed_sub: todos_changed_sub,
            _bounds_sub: bounds_sub,
            _settings_sub: settings_sub,
            _appearance_sub: appearance_sub,
        }
    }

    /// 提交防抖保存：每次变更以全量内容重建该 key 的计时（ADR-0001）。
    ///
    /// 空草稿（未晋升）无保存价值：取消旧槽并回到未触碰态，免于落出空笔记
    /// 或过期内容；已晋升为 `Note(id)` 的笔记允许存空（"已落库不删"）。
    fn dispatch_save(&mut self, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        if title.is_empty()
            && content.is_empty()
            && let NoteKey::Draft(_) = &self.key
        {
            let key = self.key.clone();
            self.notes.update(cx, |state, _| state.cancel(&key));
            self.status = SaveIndicator::Untouched;
            cx.notify();
            return;
        }
        let key = self.key.clone();
        self.notes.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key,
                    title,
                    content,
                },
                cx,
            )
        });
        // 保存失败持续提示直至再次成功，重试期间不降级为"未保存"
        if self.status != SaveIndicator::Failed {
            self.status = SaveIndicator::Pending;
        }
        cx.notify();
    }

    /// 草稿晋升：记录笔记 id 并把防抖槽身份切换为 `Note(id)`（§5.6 id 回填闭环）。
    ///
    /// 落库完成的消息派发若与输入交错，旧 `Draft` 槽可能已带过期内容重建计时，
    /// 到期会以 Draft 身份插入重复笔记；晋升时 cancel 旧槽并按当前全量内容
    /// 重新入队，掐断该路径。
    fn promote(&mut self, id: i64, cx: &mut Context<Self>) {
        let draft_key = std::mem::replace(&mut self.key, NoteKey::Note(id));
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        let key = self.key.clone();
        self.notes.update(cx, |state, cx| {
            state.cancel(&draft_key);
            state.save(
                NoteDraft {
                    key,
                    title,
                    content,
                },
                cx,
            );
        });
    }

    /// 按钮与原生关闭共用判断：空磁贴取消待写槽，非空先同步保存，失败则留窗。
    fn can_close(&mut self, cx: &mut Context<Self>) -> bool {
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        if title.is_empty() && content.is_empty() {
            let key = self.key.clone();
            self.notes.update(cx, |state, _| state.cancel(&key));
            true
        } else {
            let key = self.key.clone();
            let status = self.notes.update(cx, |state, cx| state.flush_now(key, cx));
            if status == Some(SaveStatus::Failed) {
                self.status = SaveIndicator::Failed;
                cx.notify();
                false
            } else {
                true
            }
        }
    }

    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_close(cx) {
            window.remove_window();
        }
    }

    /// 置顶切换：PopUp 自带置顶，取消 / 恢复须 `SetWindowPos`；失败时状态回滚。
    fn toggle_topmost(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = !self.topmost;
        if win32::set_topmost(window, target) {
            self.topmost = target;
            cx.notify();
        }
    }

    /// 圆角区域重设：仅尺寸 / scale 变化时执行（origin-only 移动跳过，
    /// 免于每次窗口拖动都全窗口重绘）。
    fn refresh_rounded_region(&mut self, window: &mut Window) {
        let size = window.viewport_size();
        let scale = window.scale_factor();
        let dims = (size.width.as_f32(), size.height.as_f32(), scale);
        if self.region == Some(dims) {
            return;
        }
        self.region = Some(dims);
        let _ = win32::set_rounded_region(window, CORNER);
    }

    /// 磁贴尺寸记忆：把当前窗口尺寸提交设置层（防抖落盘），下次呼出
    /// 的新磁贴按此尺寸打开。拖动（仅位移）与跨显示器 scale 变化不改
    /// 变逻辑尺寸，设置层同值跳过。
    fn record_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let size = window.viewport_size();
        let settings = cx.global::<SettingsStore>().0.clone();
        settings.update(cx, |state, cx| {
            state.set_tile_size(size.width, size.height, cx)
        });
    }

    fn new_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        if !title.is_empty() || !content.is_empty() {
            let key = self.key.clone();
            let status = self.notes.update(cx, |state, cx| state.flush_now(key, cx));
            if status == Some(SaveStatus::Failed) {
                self.mode = TileMode::Editor;
                self.status = SaveIndicator::Failed;
                self.focus_editor(window, cx);
                cx.notify();
                return;
            }
        }
        let previous = self.key.clone();
        self.notes.update(cx, |state, _| state.cancel(&previous));
        self.key = NoteKey::Draft(cx.entity().entity_id());
        self.title
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.source.set_value("", window, cx);
        self.word_count = 0;
        self.status = SaveIndicator::Untouched;
        self.mode = TileMode::Editor;
        self.midnight_task = None;
        self.focus_editor(window, cx);
        cx.notify();
    }

    fn toggle_mode(&mut self, target: TileMode, window: &mut Window, cx: &mut Context<Self>) {
        let next = if self.mode == target {
            TileMode::Editor
        } else {
            target
        };
        if self.mode == TileMode::Todos {
            self.midnight_task = None;
        }
        self.mode = next;
        match next {
            TileMode::Editor => self.focus_editor(window, cx),
            TileMode::Notes => {
                self.refresh_notes(cx);
                let focus = self.query_input.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
            }
            TileMode::Todos => self.refresh_todos(cx),
        }
        cx.notify();
    }

    fn focus_editor(&self, window: &mut Window, cx: &mut App) {
        let focus = self.source.textarea().read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    fn refresh_notes(&mut self, cx: &mut Context<Self>) {
        let filter = NoteFilter {
            query: self.query_input.read(cx).value().to_string(),
            ..NoteFilter::default()
        };
        self.note_list = self
            .notes
            .read(cx)
            .notes(&filter)
            .map_err(|err| err.to_string());
        cx.notify();
    }

    fn refresh_todos(&mut self, cx: &mut Context<Self>) {
        self.todo_date = Local::now().date_naive();
        self.todo_list = self
            .todos
            .read(cx)
            .todos_between(self.todo_date, self.todo_date)
            .map_err(|err| err.to_string());
        self.schedule_midnight_refresh(cx);
        cx.notify();
    }

    fn schedule_midnight_refresh(&mut self, cx: &mut Context<Self>) {
        self.midnight_task = None;
        let now = Local::now();
        let Some(tomorrow) = now.date_naive().succ_opt() else {
            return;
        };
        let Some(midnight) = tomorrow.and_hms_opt(0, 0, 0) else {
            return;
        };
        let Some(next) = Local.from_local_datetime(&midnight).earliest() else {
            return;
        };
        let delay = next
            .signed_duration_since(now)
            .to_std()
            .unwrap_or(Duration::ZERO);
        self.midnight_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                if this.mode == TileMode::Todos {
                    this.refresh_todos(cx);
                }
            });
        }));
    }

    fn open_note(&mut self, note_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.key.clone();
        let current_status = self
            .notes
            .update(cx, |state, cx| state.flush_now(current, cx));
        if current_status == Some(SaveStatus::Failed) {
            self.mode = TileMode::Editor;
            self.status = SaveIndicator::Failed;
            self.focus_editor(window, cx);
            cx.notify();
            return;
        }
        let target_status = self
            .notes
            .update(cx, |state, cx| state.flush_now(NoteKey::Note(note_id), cx));
        if target_status == Some(SaveStatus::Failed) {
            toast::error("目标笔记保存失败，无法打开", window, cx);
            return;
        }

        let note = self
            .notes
            .read(cx)
            .notes(&NoteFilter::default())
            .map_err(|err| err.to_string())
            .map(|notes| notes.into_iter().find(|note| note.id == note_id));
        match note {
            Ok(Some(note)) => {
                self.key = NoteKey::Note(note.id);
                self.title
                    .update(cx, |state, cx| state.set_value(&note.title, window, cx));
                self.source.set_value(&note.content, window, cx);
                self.word_count = self.source.word_count(cx);
                self.status = SaveIndicator::Untouched;
                self.mode = TileMode::Editor;
                self.focus_editor(window, cx);
                cx.notify();
            }
            Ok(None) => {
                toast::error("笔记不存在", window, cx);
            }
            Err(err) => {
                toast::error(err, window, cx);
            }
        }
    }

    fn toggle_todo(&mut self, todo: Todo, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.todos.update(cx, |state, cx| state.toggle(&todo, cx));
        if let Err(err) = result {
            toast::error(err.to_string(), window, cx);
        }
    }
}

impl TileView {
    fn editor_content(&self, content_px: Pixels, border: Hsla, muted: Hsla) -> AnyElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .min_h(tokens::heights::TITLE_ROW)
                    .items_center()
                    .overflow_hidden()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        Input::new(&self.title)
                            .appearance(false)
                            .px(px(10.))
                            .font_weight(FontWeight::BOLD)
                            .h_auto()
                            .text_size(content_px),
                    ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    self.source
                        .element()
                        .appearance(false)
                        .h_full()
                        .text_size(content_px),
                ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h_6()
                    .px_3()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child(format!("{} 字", self.word_count)),
            )
            .into_any_element()
    }

    fn notes_content(&self, weak: &WeakEntity<Self>, window: &Window, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let focused = self
            .query_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let search_border = input_border(theme, focused);
        let border = theme.border;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let search = div()
            .flex_shrink_0()
            .p(tokens::spacing::SM)
            .border_b_1()
            .border_color(border)
            .child(
                Input::new(&self.query_input)
                    .with_size(Size::Medium)
                    .w_full()
                    .text_field_focus(search_border)
                    .prefix(Icon::new(IconName::Search).with_size(Size::XSmall)),
            );
        let list = match &self.note_list {
            Err(err) => v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap(tokens::spacing::SM)
                .p(tokens::spacing::MD)
                .child(div().text_color(danger).child("加载笔记列表失败"))
                .child(
                    div()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .child(err.clone()),
                )
                .into_any_element(),
            Ok(notes) if notes.is_empty() => {
                let searching = !self.query_input.read(cx).value().is_empty();
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .text_role(TextRole::Meta)
                    .text_color(muted)
                    .child(if searching {
                        "没有匹配的笔记"
                    } else {
                        "还没有笔记"
                    })
                    .into_any_element()
            }
            Ok(notes) => v_flex()
                .id("tile-notes-scroll")
                .flex_1()
                .min_h_0()
                .gap(tokens::spacing::XS)
                .p(tokens::spacing::SM)
                .overflow_y_scrollbar()
                .children(notes.iter().map(|note| self.note_row(note, weak, cx)))
                .into_any_element(),
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .child(search)
            .child(list)
            .into_any_element()
    }

    fn note_row(&self, note: &Note, weak: &WeakEntity<Self>, cx: &App) -> Stateful<Div> {
        let note_id = note.id;
        let title = if note.title.is_empty() {
            "无标题".to_string()
        } else {
            note.title.clone()
        };
        let summary = note.summary();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let hover = theme.list_hover;

        v_flex()
            .id(("tile-note", note_id as u64))
            .gap(tokens::spacing::XS)
            .px(tokens::spacing::SM)
            .py(tokens::spacing::SM)
            .rounded(tokens::radius::SM)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click({
                let weak = weak.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.open_note(note_id, window, cx));
                }
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(tokens::spacing::SM)
                    .child(note_list::note_title(title, TILE_NOTE_TITLE_FONT))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_role(TextRole::Meta)
                            .text_color(muted)
                            .child(time::relative_created_at(note.created_at)),
                    ),
            )
            .when(!summary.is_empty(), |element| {
                element.child(note_list::note_summary(
                    summary,
                    NOTE_SUMMARY_LINES,
                    TILE_NOTE_SUMMARY_FONT,
                    muted,
                ))
            })
    }

    fn todos_content(&self, weak: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let list = match &self.todo_list {
            Err(err) => v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap(tokens::spacing::SM)
                .p(tokens::spacing::MD)
                .child(div().text_color(danger).child("加载今日待办失败"))
                .child(
                    div()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .child(err.clone()),
                )
                .into_any_element(),
            Ok(todos) if todos.is_empty() => v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .text_role(TextRole::Meta)
                .text_color(muted)
                .child("今天还没有待办")
                .into_any_element(),
            Ok(todos) => v_flex()
                .id("tile-todos-scroll")
                .flex_1()
                .min_h_0()
                .gap(tokens::spacing::XS)
                .p(tokens::spacing::SM)
                .overflow_y_scrollbar()
                .children(todos.iter().map(|todo| self.todo_row(todo, weak, cx)))
                .into_any_element(),
        };
        v_flex().flex_1().min_h_0().child(list).into_any_element()
    }

    fn todo_row(&self, todo: &Todo, weak: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let border = theme.border;
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let primary_foreground = theme.primary_foreground;
        let hover = theme.list_hover;
        let done = todo.done;
        let content_px = cx.global::<SettingsStore>().0.read(cx).tile_font_px();
        let check = div()
            .flex_shrink_0()
            .size(TODO_CHECK_SIZE)
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .map(|element| {
                if done {
                    element.bg(primary).child(
                        Icon::new(IconName::Check)
                            .with_size(Size::XSmall)
                            .text_color(primary_foreground),
                    )
                } else {
                    element.border_1().border_color(border)
                }
            });
        // 单行内容与区间同行，check 圆心与文字行中心对齐（items_center）；
        // 点击整行任意处即切换完成状态。
        h_flex()
            .id(("tile-todo", todo.id as u64))
            .items_center()
            .gap(tokens::spacing::SM)
            .p(tokens::spacing::SM)
            .rounded(tokens::radius::SM)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click({
                let weak = weak.clone();
                let todo = todo.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.toggle_todo(todo.clone(), window, cx));
                }
            })
            .child(check)
            .child(note_list::single_line(todo.content.clone(), content_px).map(|element| {
                if done {
                    element.line_through().text_color(muted)
                } else {
                    element
                }
            }))
            .when_some(todo.interval, |element, (start, end)| {
                element.child(
                    div()
                        .flex_shrink_0()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .when(done, |element| element.line_through())
                        .child(time::todo_interval_label(start, end)),
                )
            })
            .into_any_element()
    }
}

impl Render for TileView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let (background, border, muted_foreground, danger, title_bar) = {
            let theme = cx.theme();
            (
                theme.background,
                theme.border,
                theme.muted_foreground,
                theme.danger,
                theme.title_bar,
            )
        };
        // 内容阅读面：标题 / 正文随小窗字号缩放（辅助小字保持固定）
        let content_px = cx.global::<SettingsStore>().0.read(cx).tile_font_px();

        // 保存状态文案（兼拖拽区内容；区域内无交互元素，§9 条目 2）
        let status_label = match self.status {
            SaveIndicator::Untouched => None,
            SaveIndicator::Pending => Some("未保存"),
            SaveIndicator::Saved => Some("已保存"),
            SaveIndicator::Failed => Some("保存失败"),
        };

        let topbar = h_flex()
            .items_center()
            .gap_1()
            .px_2()
            .h_9()
            .flex_shrink_0()
            // 顶栏：以 title_bar token 着色（色系主题下为深调 chrome）
            .bg(title_bar)
            .border_b_1()
            .border_color(border)
            .child(
                Button::new("tile-new")
                    .ghost()
                    .compact()
                    .icon(IconName::Plus)
                    .tooltip("新建笔记")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.new_draft(window, cx));
                        }
                    }),
            )
            .child(
                Button::new("tile-notes")
                    .ghost()
                    .compact()
                    .icon(IconName::List)
                    .selected(self.mode == TileMode::Notes)
                    .tooltip("笔记列表")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.toggle_mode(TileMode::Notes, window, cx)
                            });
                        }
                    }),
            )
            .child(
                Button::new("tile-todos")
                    .ghost()
                    .compact()
                    .icon(IconName::CircleCheckBig)
                    .selected(self.mode == TileMode::Todos)
                    .tooltip("今日待办")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.toggle_mode(TileMode::Todos, window, cx)
                            });
                        }
                    }),
            )
            .child(
                h_flex()
                    .flex_1()
                    .h_full()
                    .min_w(px(24.))
                    .justify_center()
                    .window_control_area(WindowControlArea::Drag)
                    .when_some(status_label, |el, label| {
                        el.child(
                            div()
                                .text_size(px(11.))
                                .text_color(if self.status == SaveIndicator::Failed {
                                    danger
                                } else {
                                    muted_foreground
                                })
                                .child(label),
                        )
                    }),
            )
            .child(
                Button::new("tile-settings")
                    .ghost()
                    .compact()
                    .icon(IconName::Settings)
                    .tooltip("打开主窗口")
                    .on_click(|_, _, cx| crate::windows::main::open(cx)),
            )
            .child(
                Button::new("tile-topmost")
                    .ghost()
                    .compact()
                    .icon(if self.topmost {
                        IconName::PinOff
                    } else {
                        IconName::Pin
                    })
                    .tooltip(if self.topmost {
                        "取消置顶"
                    } else {
                        "置顶"
                    })
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.toggle_topmost(window, cx));
                        }
                    }),
            )
            .child(
                Button::new("tile-close")
                    .ghost()
                    .compact()
                    .icon(IconName::X)
                    .tooltip("关闭")
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.request_close(window, cx));
                        }
                    }),
            );

        let content = match self.mode {
            TileMode::Editor => self.editor_content(content_px, border, muted_foreground),
            TileMode::Notes => self.notes_content(&weak, window, cx),
            TileMode::Todos => self.todos_content(&weak, cx),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(background)
            .rounded(px(CORNER))
            .border_1()
            .border_color(border)
            .overflow_hidden()
            .child(topbar)
            .child(content)
            // toast 浮层：deferred 图层，盖过弹窗与弹层（架构 §5.10）
            .child(self.toast.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{SaveIndicator, TileMode, TileView};
    use crate::editor::ImageStore;
    use crate::notes::NotesStore;
    use crate::notes::{NoteDraft, NoteKey, NotesState, SaveStatus};
    use crate::settings::SettingsState;
    use crate::settings::SettingsStore;
    use crate::todos::{TodosState, TodosStore};
    use crate::widgets::toast::{ToastState, ToastStore};
    use chrono::Local;
    use fragmenta_core::{
        db::Db,
        media::Images,
        model::{Note, NoteFilter, Todo},
        settings::Settings,
    };
    use gpui_kit::{
        AnyWindowHandle, AppContext as _, Entity, Focusable as _, TestAppContext,
        VisualTestContext, WindowOptions,
    };
    use std::{path::PathBuf, sync::Arc};

    fn open_tile(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<TileView>, Arc<Db>) {
        cx.update(gpui_kit::init);
        let db = Arc::new(Db::open_in_memory().unwrap());
        let notes = cx.new(|_| NotesState::new(db.clone()));
        let todos = cx.new(|_| TodosState::new(db.clone()));
        let settings = cx.new(|cx| {
            SettingsState::new(
                Settings::default(),
                PathBuf::from("temp/tile-test-settings.json"),
                db.clone(),
                cx,
            )
        });
        let (handle, tile) = cx.update(|cx| {
            cx.set_global(NotesStore(notes));
            cx.set_global(TodosStore(todos));
            cx.set_global(SettingsStore(settings));
            let toast = cx.new(|_| ToastState::new());
            cx.set_global(ToastStore(toast));
            cx.set_global(ImageStore(Images::new(PathBuf::from(
                "temp/tile-test-imgs",
            ))));
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| TileView::new(window, cx))
            })
            .unwrap()
        });
        (handle, tile, db)
    }

    fn insert_note(db: &Db, title: &str, content: &str) -> Note {
        let now = Local::now();
        let mut note = Note {
            id: 0,
            title: title.into(),
            content: content.into(),
            category: None,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        db.upsert_note(&mut note).unwrap();
        note
    }

    fn insert_todo(db: &Db, content: &str, date: chrono::NaiveDate) -> Todo {
        let now = Local::now();
        let mut todo = Todo {
            id: 0,
            content: content.into(),
            date,
            interval: None,
            done: false,
            created_at: now,
            updated_at: now,
        };
        db.upsert_todo(&mut todo).unwrap();
        todo
    }

    #[gpui_kit::test]
    fn native_close_saves_nonempty_tile(cx: &mut TestAppContext) {
        let (handle, tile, db) = open_tile(cx);
        cx.update_window(handle, |_, window, cx| {
            tile.update(cx, |tile, cx| {
                tile.title.update(cx, |input, cx| {
                    input.set_value("原生关闭前保存", window, cx)
                });
                tile.dispatch_save(cx);
            });
        })
        .unwrap();
        let mut visual = VisualTestContext::from_window(handle, cx);
        assert!(visual.simulate_close());
        let notes = db.notes(&NoteFilter::default()).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "原生关闭前保存");
    }

    #[gpui_kit::test]
    fn native_close_failure_keeps_window_and_content(cx: &mut TestAppContext) {
        let (handle, tile, _) = open_tile(cx);
        cx.update_window(handle, |_, window, cx| {
            tile.update(cx, |tile, cx| {
                tile.key = NoteKey::Note(1);
                tile.title.update(cx, |input, cx| {
                    input.set_value("保存失败仍保留", window, cx)
                });
                tile.dispatch_save(cx);
            });
        })
        .unwrap();
        let mut visual = VisualTestContext::from_window(handle, cx);
        assert!(!visual.simulate_close());
        assert!(cx.windows().contains(&handle));
        tile.read_with(cx, |tile, cx| {
            assert!(tile.status == SaveIndicator::Failed);
            assert_eq!(tile.title.read(cx).value().as_str(), "保存失败仍保留");
        });
    }

    #[gpui_kit::test]
    fn native_close_empty_tile_cancels_pending(cx: &mut TestAppContext) {
        let (handle, tile, db) = open_tile(cx);
        tile.update(cx, |tile, cx| {
            let key = tile.key.clone();
            tile.notes.update(cx, |state, cx| {
                state.save(
                    NoteDraft {
                        key,
                        title: "过期待写内容".into(),
                        content: String::new(),
                    },
                    cx,
                )
            });
        });
        let mut visual = VisualTestContext::from_window(handle, cx);
        assert!(visual.simulate_close());
        let notes = cx.update(|cx| cx.global::<NotesStore>().0.clone());
        assert_eq!(
            notes.update(cx, |state, cx| state.flush_all(cx)),
            SaveStatus::Saved
        );
        assert!(db.notes(&NoteFilter::default()).unwrap().is_empty());
    }

    #[gpui_kit::test]
    fn notes_search_covers_title_and_content_and_survives_mode_switch(cx: &mut TestAppContext) {
        let (handle, tile, db) = open_tile(cx);
        insert_note(&db, "命中标题", "正文");
        insert_note(&db, "其他", "正文命中");
        insert_note(&db, "无关", "无关");
        cx.update_window(handle, |_, window, cx| {
            tile.update(cx, |tile, cx| {
                tile.toggle_mode(TileMode::Notes, window, cx);
                assert!(
                    tile.query_input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                );
                tile.query_input.update(cx, |input, cx| {
                    input.set_value("命中", window, cx);
                });
                tile.refresh_notes(cx);
                assert_eq!(tile.note_list.as_ref().unwrap().len(), 2);

                tile.toggle_mode(TileMode::Notes, window, cx);
                assert_eq!(tile.mode, TileMode::Editor);
                assert!(
                    tile.source
                        .textarea()
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                );
                tile.toggle_mode(TileMode::Notes, window, cx);
                assert_eq!(tile.query_input.read(cx).value().as_str(), "命中");
                assert_eq!(tile.note_list.as_ref().unwrap().len(), 2);
            });
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn open_note_flushes_pending_writes_and_loads_latest_content(cx: &mut TestAppContext) {
        let (handle, tile, db) = open_tile(cx);
        let target = insert_note(&db, "目标旧标题", "目标旧正文");
        cx.update_window(handle, |_, window, cx| {
            tile.update(cx, |tile, cx| {
                tile.title.update(cx, |input, cx| {
                    input.set_value("当前草稿", window, cx);
                });
                tile.source.set_value("当前正文", window, cx);
                tile.dispatch_save(cx);
                tile.notes.update(cx, |state, cx| {
                    state.save(
                        NoteDraft {
                            key: NoteKey::Note(target.id),
                            title: "目标新标题".into(),
                            content: "目标新正文".into(),
                        },
                        cx,
                    );
                });
                tile.toggle_mode(TileMode::Notes, window, cx);
                tile.open_note(target.id, window, cx);

                assert_eq!(tile.mode, TileMode::Editor);
                assert_eq!(tile.key, NoteKey::Note(target.id));
                assert_eq!(tile.title.read(cx).value().as_str(), "目标新标题");
                assert_eq!(tile.source.value(cx), "目标新正文");
            });
        })
        .unwrap();
        let notes = db.notes(&NoteFilter::default()).unwrap();
        assert!(
            notes
                .iter()
                .any(|note| note.title == "当前草稿" && note.content == "当前正文")
        );
        let target = notes.iter().find(|note| note.id == target.id).unwrap();
        assert_eq!(target.title, "目标新标题");
        assert_eq!(target.content, "目标新正文");
    }

    #[gpui_kit::test]
    fn today_todos_toggle_done_and_cancel_midnight_task_on_exit(cx: &mut TestAppContext) {
        let (handle, tile, db) = open_tile(cx);
        let today = Local::now().date_naive();
        let todo = insert_todo(&db, "今日事项", today);
        insert_todo(&db, "昨日事项", today.pred_opt().unwrap());
        cx.update_window(handle, |_, window, cx| {
            tile.update(cx, |tile, cx| {
                tile.toggle_mode(TileMode::Todos, window, cx);
                assert_eq!(tile.todo_date, today);
                assert_eq!(tile.todo_list.as_ref().unwrap().len(), 1);
                assert!(tile.midnight_task.is_some());
                tile.toggle_todo(todo.clone(), window, cx);
            });
        })
        .unwrap();
        cx.executor().run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            tile.update(cx, |tile, cx| {
                assert!(tile.todo_list.as_ref().unwrap()[0].done);
                let done = tile.todo_list.as_ref().unwrap()[0].clone();
                tile.toggle_todo(done, window, cx);
            });
        })
        .unwrap();
        cx.executor().run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            tile.update(cx, |tile, cx| {
                assert!(!tile.todo_list.as_ref().unwrap()[0].done);
                tile.toggle_mode(TileMode::Todos, window, cx);
                assert_eq!(tile.mode, TileMode::Editor);
                assert!(tile.midnight_task.is_none());
            });
        })
        .unwrap();
        assert!(!db.todos_between(today, today).unwrap()[0].done);
    }
}
