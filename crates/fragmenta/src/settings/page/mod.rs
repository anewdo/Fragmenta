//! 设置页：常驻实体装配与页面骨架；各组内容见同目录子模块。

mod appearance;
mod common;
mod data;
mod fonts;
mod hotkey;
mod system;

use fragmenta_core::settings::{
    FONT_SIZE_MAX_PX, FONT_SIZE_MIN_PX, LINE_HEIGHT_MAX, LINE_HEIGHT_MIN,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::combobox::{ComboboxEvent, ComboboxState};
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme as _, IndexPath};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::SettingsStore;
use crate::ui::layout::{LayoutClass, content_column, page_scroll};
use crate::ui::tokens;

use common::setting_section;
use fonts::FontChoice;

/// 字体下拉状态类型别名，避免字段声明处堆叠泛型。
type FontCombobox = ComboboxState<SearchableVec<SharedString>>;

/// 设置页视图：常驻于主窗口 shell，订阅设置广播刷新展示。
pub(crate) struct SettingsPage {
    _subscription: Subscription,
    /// 窗口尺寸变化：档位留白随视口宽度切换。
    _bounds_sub: Subscription,
    /// 编辑器字号滑杆与拖动中的实时预览值。
    editor_font_slider: Entity<SliderState>,
    editor_font_preview: f32,
    _editor_font_sub: Subscription,
    /// 编辑器字体下拉。
    editor_font_combo: Entity<FontCombobox>,
    _editor_font_family_sub: Subscription,
    /// 编辑器行距滑杆与拖动中的实时预览值。
    editor_line_slider: Entity<SliderState>,
    editor_line_preview: f32,
    _editor_line_sub: Subscription,
    /// 小窗字号滑杆与拖动中的实时预览值。
    tile_font_slider: Entity<SliderState>,
    tile_font_preview: f32,
    _tile_font_sub: Subscription,
    /// 小窗字体下拉。
    tile_font_combo: Entity<FontCombobox>,
    _tile_font_family_sub: Subscription,
    /// 小窗行距滑杆与拖动中的实时预览值。
    tile_line_slider: Entity<SliderState>,
    tile_line_preview: f32,
    _tile_line_sub: Subscription,
    /// 热键录入框焦点：监听态接收按键的载体。
    hotkey_focus: FocusHandle,
    /// 热键录入框是否处于监听态（点击进入，捕获完成 / 取消退出）。
    hotkey_listening: bool,
    /// 监听中和弦进行中已按下的修饰键展示（如 "Ctrl + Alt"）。
    hotkey_pending_mods: Option<String>,
    /// 热键录入错误提示（红字展示）。
    hotkey_error: Option<&'static str>,
    /// 数据目录迁移错误提示（红字展示）。
    data_dir_error: Option<String>,
    /// 开机自启动开关错误提示（红字展示）。
    autostart_error: Option<String>,
}

impl SettingsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = cx.global::<SettingsStore>().0.clone();
        let subscription = cx.subscribe(&settings, |_, _, _, cx| {
            // 设置变更（含本页操作）→ 刷新展示
            cx.notify();
        });
        let bounds_sub = cx.observe_window_bounds(window, |_, _, cx| cx.notify());
        let (editor_px, tile_px, editor_family, tile_family, editor_line, tile_line) = {
            let state = settings.read(cx);
            (
                state.editor_font_px().as_f32(),
                state.tile_font_px().as_f32(),
                state.editor_font_family().map(str::to_string),
                state.tile_font_family().map(str::to_string),
                state.editor_line_height(),
                state.tile_line_height(),
            )
        };
        // 字体候选：仅常用阅读字体，按本机安装情况过滤（启动时读取一次）
        let editor_choices = fonts::choices(cx, editor_family.as_deref());
        let tile_choices = fonts::choices(cx, tile_family.as_deref());
        let editor_font_index = fonts::label_index(&editor_choices, editor_family.as_deref());
        let tile_font_index = fonts::label_index(&tile_choices, tile_family.as_deref());
        let editor_font_combo = font_combo(window, cx, &editor_choices, editor_font_index);
        let tile_font_combo = font_combo(window, cx, &tile_choices, tile_font_index);
        let editor_font_family_sub = cx.subscribe(
            &editor_font_combo,
            move |_, _, event: &ComboboxEvent<SearchableVec<SharedString>>, cx| {
                let ComboboxEvent::Confirm(values) = event else {
                    return;
                };
                let Some(label) = values.first() else {
                    return;
                };
                let Some(family) = fonts::family_of(&editor_choices, label.as_str()) else {
                    return;
                };
                let store = cx.global::<SettingsStore>().0.clone();
                store.update(cx, |state, cx| state.set_editor_font_family(family, cx));
            },
        );
        let tile_font_family_sub = cx.subscribe(
            &tile_font_combo,
            move |_, _, event: &ComboboxEvent<SearchableVec<SharedString>>, cx| {
                let ComboboxEvent::Confirm(values) = event else {
                    return;
                };
                let Some(label) = values.first() else {
                    return;
                };
                let Some(family) = fonts::family_of(&tile_choices, label.as_str()) else {
                    return;
                };
                let store = cx.global::<SettingsStore>().0.clone();
                store.update(cx, |state, cx| state.set_tile_font_family(family, cx));
            },
        );
        let editor_font_slider = cx.new(|_| font_slider(editor_px));
        let tile_font_slider = cx.new(|_| font_slider(tile_px));
        let editor_line_slider = cx.new(|_| line_height_slider(editor_line));
        let tile_line_slider = cx.new(|_| line_height_slider(tile_line));
        // 拖动中只刷新本页示例；松开后落库并广播到编辑器 / 磁贴
        let editor_font_sub = cx.subscribe(
            &editor_font_slider,
            |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    this.editor_font_preview = value.end();
                    cx.notify();
                }
                SliderEvent::Release(value) => {
                    let store = cx.global::<SettingsStore>().0.clone();
                    store.update(cx, |state, cx| {
                        state.set_editor_font_px(px(value.end()), cx)
                    });
                }
            },
        );
        let tile_font_sub =
            cx.subscribe(
                &tile_font_slider,
                |this, _, event: &SliderEvent, cx| match event {
                    SliderEvent::Change(value) => {
                        this.tile_font_preview = value.end();
                        cx.notify();
                    }
                    SliderEvent::Release(value) => {
                        let store = cx.global::<SettingsStore>().0.clone();
                        store.update(cx, |state, cx| state.set_tile_font_px(px(value.end()), cx));
                    }
                },
            );
        let editor_line_sub = cx.subscribe(
            &editor_line_slider,
            |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    this.editor_line_preview = value.end();
                    cx.notify();
                }
                SliderEvent::Release(value) => {
                    let store = cx.global::<SettingsStore>().0.clone();
                    store.update(cx, |state, cx| {
                        state.set_editor_line_height(value.end(), cx)
                    });
                }
            },
        );
        let tile_line_sub =
            cx.subscribe(
                &tile_line_slider,
                |this, _, event: &SliderEvent, cx| match event {
                    SliderEvent::Change(value) => {
                        this.tile_line_preview = value.end();
                        cx.notify();
                    }
                    SliderEvent::Release(value) => {
                        let store = cx.global::<SettingsStore>().0.clone();
                        store.update(cx, |state, cx| state.set_tile_line_height(value.end(), cx));
                    }
                },
            );
        Self {
            _subscription: subscription,
            _bounds_sub: bounds_sub,
            editor_font_slider,
            editor_font_preview: editor_px,
            _editor_font_sub: editor_font_sub,
            editor_font_combo,
            _editor_font_family_sub: editor_font_family_sub,
            editor_line_slider,
            editor_line_preview: editor_line,
            _editor_line_sub: editor_line_sub,
            tile_font_slider,
            tile_font_preview: tile_px,
            _tile_font_sub: tile_font_sub,
            tile_font_combo,
            _tile_font_family_sub: tile_font_family_sub,
            tile_line_slider,
            tile_line_preview: tile_line,
            _tile_line_sub: tile_line_sub,
            hotkey_focus: cx.focus_handle(),
            hotkey_listening: false,
            hotkey_pending_mods: None,
            hotkey_error: None,
            data_dir_error: None,
            autostart_error: None,
        }
    }
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let foreground = cx.theme().foreground;
        let content = content_column(tokens::widths::SETTINGS_CONTENT)
            .gap(tokens::spacing::XXL)
            .child(setting_section(
                IconName::SwatchBook,
                "外观",
                foreground,
                self.appearance_group(cx),
            ))
            .child(setting_section(
                IconName::Keyboard,
                "快捷键",
                foreground,
                self.hotkey_group(cx),
            ))
            .child(setting_section(
                IconName::Database,
                "数据",
                foreground,
                self.data_group(cx),
            ))
            .child(setting_section(
                IconName::Settings2,
                "系统",
                foreground,
                self.system_group(cx),
            ));
        page_scroll(
            LayoutClass::from_width(window.viewport_size().width),
            content,
        )
    }
}

/// 字体下拉实体：选项为简短阅读字体列表，无需搜索框。
fn font_combo(
    window: &mut Window,
    cx: &mut Context<SettingsPage>,
    choices: &[FontChoice],
    index: usize,
) -> Entity<FontCombobox> {
    let labels = fonts::labels(choices);
    cx.new(|cx| {
        ComboboxState::new(
            SearchableVec::new(labels),
            vec![IndexPath::new(index)],
            window,
            cx,
        )
    })
}

/// 字号滑杆：整数步进，范围由 core 设置常量统一约束。
fn font_slider(value: f32) -> SliderState {
    SliderState::new()
        .min(FONT_SIZE_MIN_PX)
        .max(FONT_SIZE_MAX_PX)
        .step(1.)
        .default_value(value.clamp(FONT_SIZE_MIN_PX, FONT_SIZE_MAX_PX))
}

/// 行距滑杆：0.05 倍步进，范围由 core 设置常量统一约束。
fn line_height_slider(value: f32) -> SliderState {
    SliderState::new()
        .min(LINE_HEIGHT_MIN)
        .max(LINE_HEIGHT_MAX)
        .step(0.05)
        .default_value(value.clamp(LINE_HEIGHT_MIN, LINE_HEIGHT_MAX))
}
