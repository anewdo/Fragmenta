//! Markdown 编辑器组装（架构 §5.9）：笔记页右栏，标题 + 正文 + 三模式。
//!
//! - 模式 `Source | Preview | Split`：Source = 源文本输入组件；
//!   Preview = markdown + 图片路径插件；Split 双栏各自独立滚动（ADR-0003）
//! - 编辑防抖保存与磁贴共用 `NotesState`；编辑器只以 `Note(id)` 为防抖
//!   key（笔记必来自列表选中，无草稿晋升路径）
//! - 订阅矩阵（§5.6）：不订阅 `Changed`（编辑器是当前笔记内容的事实
//!   来源），只响应携带自己 key 的 `Saved` 更新保存状态
//! - 载入 / 清空由笔记页驱动：页在选中前先 `flush_now` 目标 key 并重拉
//!   快照，保证载入内容包含磁贴 / 本编辑器的最新待写内容；正打开的笔记
//!   被删除时页调用 `clear`（cancel 待写槽，免于过期 upsert 报错）

use fragmenta_core::media::Images;
use fragmenta_core::model::Note;

use gpui_kit::assets::IconName;
use gpui_kit::base::text::TextView;
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::image_path::ImagePathPlugin;
use super::source::SourceText;
use crate::app::{ImageStore, NotesStore};
use crate::state::{Event, NoteDraft, NoteKey, NotesState, SaveStatus};
use crate::widgets::ResizableSplit;

/// 编辑器模式（架构 §5.9）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    /// 源文本编辑。
    Source,
    /// markdown 渲染预览。
    Preview,
    /// 双栏：左源文右预览，各自独立滚动（ADR-0003）。
    Split,
}

/// 保存状态（与磁贴同构：补充 UI 侧"未触碰 / 防抖中"两态）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveIndicator {
    /// 未载入笔记或尚无任何输入。
    Untouched,
    /// 有输入待落库（防抖计时段）。
    Pending,
    /// 最近一次落库成功。
    Saved,
    /// 最近一次落库失败；持续显示直至再次成功。
    Failed,
}

/// 笔记编辑器：笔记页右栏，标题 + 正文 + 模式切换 + 防抖保存闭环。
pub struct NoteEditor {
    /// 当前载入的笔记 id（None = 空态占位）。
    note_id: Option<i64>,
    notes: Entity<NotesState>,
    images: Images,
    mode: EditorMode,
    title: Entity<InputState>,
    source: SourceText,
    status: SaveIndicator,
    word_count: usize,
    _title_sub: Subscription,
    _word_sub: Subscription,
    _saved_sub: Subscription,
}

impl NoteEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let notes = cx.global::<NotesStore>().0.clone();
        let images = cx.global::<ImageStore>().0.clone();
        let source = SourceText::new(images.clone(), "写点什么……", window, cx);
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("标题"));

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
        // 订阅矩阵（§5.6）：只响应携带自己 key 的 Saved，不订阅 Changed
        let saved_sub = cx.subscribe(&notes, |this, _, event: &Event, cx| {
            if let Event::Saved { key, status, .. } = event
                && let NoteKey::Note(id) = key
                && this.note_id == Some(*id)
            {
                this.status = match status {
                    SaveStatus::Saved => SaveIndicator::Saved,
                    SaveStatus::Failed => SaveIndicator::Failed,
                };
                cx.notify();
            }
        });

        Self {
            note_id: None,
            notes,
            images,
            mode: EditorMode::Split,
            title,
            source,
            status: SaveIndicator::Untouched,
            word_count: 0,
            _title_sub: title_sub,
            _word_sub: word_sub,
            _saved_sub: saved_sub,
        }
    }

    /// 当前载入的笔记 id。
    pub fn note_id(&self) -> Option<i64> {
        self.note_id
    }

    /// 当前编辑器模式（切换按钮组由页面标题栏承载，见 `windows/main`）。
    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    /// 切换编辑器模式。
    pub fn set_mode(&mut self, mode: EditorMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    /// 载入笔记：整体替换标题与正文（调用方——笔记页——已先落目标 key 的
    /// pending 并重拉快照，内容为库中最新）。
    ///
    /// `set_value` 不发 Change（字数回调不触发），字数在此显式刷新；
    /// 保存状态回到未触碰态。
    pub fn load(&mut self, note: &Note, window: &mut Window, cx: &mut Context<Self>) {
        self.note_id = Some(note.id);
        self.title
            .update(cx, |state, cx| state.set_value(&note.title, window, cx));
        self.source.set_value(&note.content, window, cx);
        self.word_count = self.source.word_count(cx);
        self.status = SaveIndicator::Untouched;
        cx.notify();
    }

    /// 清空编辑器：正打开的笔记被删除时由笔记页调用。
    ///
    /// 该笔记的待写槽一并 cancel——笔记已不存在，到期 upsert 只会落出
    /// `NoteNotFound` 的虚警失败事件。
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.note_id.take() {
            self.notes
                .update(cx, |state, _| state.cancel(&NoteKey::Note(id)));
        }
        self.title
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.source.set_value("", window, cx);
        self.word_count = 0;
        self.status = SaveIndicator::Untouched;
        cx.notify();
    }

    /// 提交防抖保存：每次变更以全量内容重建该 key 的计时（ADR-0001）。
    ///
    /// 编辑器只编辑已落库笔记（key 恒为 `Note(id)`），允许存空
    /// （"已落库不删"，与磁贴晋升后的语义一致）。
    fn dispatch_save(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.note_id else {
            return;
        };
        let title = self.title.read(cx).value().to_string();
        let content = self.source.value(cx);
        self.notes.update(cx, |state, cx| {
            state.save(
                NoteDraft {
                    key: NoteKey::Note(id),
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

    /// markdown 预览：图片路径插件落地 §8 三条规则，滚动独立（ADR-0003）。
    fn preview_element(&self, cx: &App) -> impl IntoElement {
        let content = self.source.value(cx);
        let images = self.images.clone();
        div().size_full().p_3().child(
            TextView::markdown("editor-preview", content)
                .plugin(ImagePathPlugin { images })
                .scrollable(true)
                .size_full(),
        )
    }
}

impl Render for NoteEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (background, border, muted, danger) = {
            let theme = cx.theme();
            (
                theme.background,
                theme.border,
                theme.muted_foreground,
                theme.danger,
            )
        };

        // 空态：未选中笔记
        let Some(_) = self.note_id else {
            return v_flex()
                .size_full()
                .bg(background)
                .items_center()
                .justify_center()
                .child(
                    Empty::new().header(
                        EmptyHeader::new()
                            .media(
                                EmptyMedia::new()
                                    .with_variant(EmptyMediaVariant::Icon)
                                    .child(Icon::new(IconName::FileText)),
                            )
                            .title(EmptyTitle::new().child("未选择笔记"))
                            .description(
                                EmptyDescription::new().child("点击左侧卡片，在此阅读与编辑笔记"),
                            ),
                    ),
                )
                .into_any_element();
        };

        let status_label = match self.status {
            SaveIndicator::Untouched => None,
            SaveIndicator::Pending => Some("未保存"),
            SaveIndicator::Saved => Some("已保存"),
            SaveIndicator::Failed => Some("保存失败"),
        };

        // 模式切换按钮组由主窗口标题栏承载（与页面标题同行，见 windows/main）

        let title_row = h_flex()
            .flex_shrink_0()
            .h_9()
            .items_center()
            .overflow_hidden()
            .border_b_1()
            .border_color(border)
            .child(
                Input::new(&self.title)
                    .appearance(false)
                    .px(px(12.))
                    .font_weight(FontWeight::MEDIUM),
            );

        // 正文区：三模式（Split 内分栏同样可拖动，两栏滚动独立）
        let source = self
            .source
            .element()
            .appearance(false)
            .h_full()
            .into_any_element();
        let body = match self.mode {
            EditorMode::Source => div().size_full().child(source).into_any_element(),
            EditorMode::Preview => self.preview_element(cx).into_any_element(),
            EditorMode::Split => ResizableSplit::new(
                "editor-split",
                source,
                self.preview_element(cx).into_any_element(),
            )
            .into_any_element(),
        };

        v_flex()
            .size_full()
            .min_w_0()
            .bg(background)
            .child(title_row)
            .child(div().flex_1().min_h_0().min_w_0().child(body))
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h_6()
                    .px_3()
                    .gap_2()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child(format!("{} 字", self.word_count))
                    .when_some(status_label, |el, label| {
                        el.child(
                            div()
                                .text_color(if self.status == SaveIndicator::Failed {
                                    danger
                                } else {
                                    muted
                                })
                                .child(label),
                        )
                    }),
            )
            .into_any_element()
    }
}
