//! 源文本输入组件：`TextareaState` 封装 + 图片粘贴 + 字数统计

use fragmenta_core::media::Images;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::{
    App, AppContext, ClipboardEntry, Context, Entity, SharedString, Subscription, Window,
};

/// 源文本输入组件。
///
/// 薄封装 [`TextareaState`]：宿主经 [`SourceText::element`] 拿到挂了粘贴钩子的
/// [`Textarea`] 渲染元素；图片粘贴 → [`Images::import`] → 光标处插入
/// `![图片](imgs/{hash}.{ext})`（架构 §8），文本粘贴走默认路径。
pub struct SourceText {
    textarea: Entity<TextareaState>,
    images: Images,
}

impl SourceText {
    /// 创建组件。`images` 为图库（应用层固定注入 `{exe_dir}/imgs`）。
    pub fn new<T: 'static>(images: Images, window: &mut Window, cx: &mut Context<T>) -> Self {
        Self {
            textarea: cx.new(|cx| TextareaState::new(window, cx)),
            images,
        }
    }

    /// 渲染元素（已挂图片粘贴钩子，图与文本并存时图片优先）。
    pub fn element(&self) -> Textarea {
        let weak_textarea = self.textarea.downgrade();
        let images = self.images.clone();
        Textarea::new(&self.textarea).on_paste(move |item, window, cx| {
            let Some(image) = item.entries().iter().find_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.clone()),
                _ => None,
            }) else {
                // 无图片条目：落回默认文本粘贴
                return false;
            };
            let Ok(reference) = images.import(&image.bytes, image.format.extension()) else {
                // 图库导入失败：同样落回默认路径
                return false;
            };
            let Some(textarea) = weak_textarea.upgrade() else {
                return false;
            };
            textarea.update(cx, |state, cx| {
                state.insert(format!("![图片]({reference})"), window, cx);
            });
            true
        })
    }

    /// 光标处插入文本（发出 Change 事件，字数回调随之触发）。
    pub fn insert(&self, text: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
        self.textarea
            .update(cx, |state, cx| state.insert(text, window, cx));
    }

    /// 整体设置内容。不发出 Change（字数回调不触发），宿主载入既有笔记后
    /// 需经 [`SourceText::word_count`] 自行刷新字数显示。
    pub fn set_value(&self, value: &str, window: &mut Window, cx: &mut App) {
        self.textarea
            .update(cx, |state, cx| state.set_value(value, window, cx));
    }

    /// 当前内容。
    pub fn value(&self, cx: &App) -> String {
        self.textarea.read(cx).value().to_string()
    }

    /// 当前字数（按 Unicode 字符计）。
    pub fn word_count(&self, cx: &App) -> usize {
        self.textarea.read(cx).value().chars().count()
    }

    /// 订阅字数变化：文本变更（含 [`SourceText::insert`]）时回调最新字数。
    /// 返回的 [`Subscription`] 由宿主持有，丢弃即退订。
    pub fn on_word_count<T: 'static>(
        &self,
        cx: &mut Context<T>,
        mut callback: impl FnMut(&mut T, usize, &mut Context<T>) + 'static,
    ) -> Subscription {
        let textarea = self.textarea.clone();
        let reader = textarea.clone();
        cx.subscribe(&textarea, move |this, _entity, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let count = reader.read(cx).value().chars().count();
                callback(this, count, cx);
            }
        })
    }

    /// 内层 [`TextareaState`] 实体，供宿主做 placeholder 等细粒度定制。
    pub fn textarea(&self) -> &Entity<TextareaState> {
        &self.textarea
    }
}
