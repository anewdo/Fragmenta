//! 排版控件：编辑器 / 小窗两组字体下拉、字号与行距滑杆与实时预览值。

use fragmenta_core::settings::{
    FONT_SIZE_MAX_PX, FONT_SIZE_MIN_PX, LINE_HEIGHT_MAX, LINE_HEIGHT_MIN,
};
use gpui_kit::component::IndexPath;
use gpui_kit::component::combobox::{ComboboxEvent, ComboboxState};
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::SettingsPage;
use super::font_choices::{self, FontChoice, FontChoices};
use super::font_picker::FontPickerHost;
use crate::settings::SettingsStore;

/// 字体下拉状态类型别名，避免字段声明处堆叠泛型。
pub(super) type FontCombobox = ComboboxState<FontChoices>;

/// 已同步到两个字体下拉的候选来源快照：自定义字体或当前字体变化时重建选项。
#[derive(Default, PartialEq, Eq)]
struct FontSnapshot {
    custom: Vec<String>,
    editor: Option<String>,
    tile: Option<String>,
}

/// 两套排版控件与会话内状态：常驻于设置页，设置广播触发下拉候选同步。
pub(super) struct TypographyControls {
    /// 编辑器字号滑杆与拖动中的实时预览值。
    pub(super) editor_font_slider: Entity<SliderState>,
    pub(super) editor_font_preview: f32,
    /// 编辑器字体下拉。
    pub(super) editor_font_combo: Entity<FontCombobox>,
    /// 编辑器行距滑杆与拖动中的实时预览值。
    pub(super) editor_line_slider: Entity<SliderState>,
    pub(super) editor_line_preview: f32,
    /// 小窗字号滑杆与拖动中的实时预览值。
    pub(super) tile_font_slider: Entity<SliderState>,
    pub(super) tile_font_preview: f32,
    /// 小窗字体下拉。
    pub(super) tile_font_combo: Entity<FontCombobox>,
    /// 小窗行距滑杆与拖动中的实时预览值。
    pub(super) tile_line_slider: Entity<SliderState>,
    pub(super) tile_line_preview: f32,
    /// 两个字体下拉当前同步的候选来源快照。
    snapshot: FontSnapshot,
    /// 系统字体选择弹窗的挂载与生命周期。
    pub(super) font_picker: FontPickerHost,
    /// 滑杆与字体下拉的事件订阅；随控件一同释放。
    _subscriptions: Vec<Subscription>,
}

impl TypographyControls {
    pub(super) fn new(window: &mut Window, cx: &mut Context<SettingsPage>) -> Self {
        let settings = cx.global::<SettingsStore>().0.clone();
        let (
            editor_px,
            tile_px,
            editor_line,
            tile_line,
            editor_family,
            tile_family,
            editor_choices,
            tile_choices,
            custom,
        ) = {
            let state = settings.read(cx);
            let editor_family = state.editor_font_family().map(str::to_string);
            let tile_family = state.tile_font_family().map(str::to_string);
            // 字体候选：内置阅读字体全量列出（未安装项禁用并标注）+ 用户添加的系统字体
            let editor_choices = font_choices::choices(
                state.installed_font_names(),
                state.custom_font_families(),
                editor_family.as_deref(),
            );
            let tile_choices = font_choices::choices(
                state.installed_font_names(),
                state.custom_font_families(),
                tile_family.as_deref(),
            );
            (
                state.editor_font_px().as_f32(),
                state.tile_font_px().as_f32(),
                state.editor_line_height(),
                state.tile_line_height(),
                editor_family,
                tile_family,
                editor_choices,
                tile_choices,
                state.custom_font_families().to_vec(),
            )
        };
        let editor_font_index =
            font_choices::label_index(&editor_choices, editor_family.as_deref());
        let tile_font_index = font_choices::label_index(&tile_choices, tile_family.as_deref());
        let editor_font_combo = font_combo(window, cx, &editor_choices, editor_font_index);
        let tile_font_combo = font_combo(window, cx, &tile_choices, tile_font_index);
        let editor_font_slider = cx.new(|_| font_slider(editor_px));
        let tile_font_slider = cx.new(|_| font_slider(tile_px));
        let editor_line_slider = cx.new(|_| line_height_slider(editor_line));
        let tile_line_slider = cx.new(|_| line_height_slider(tile_line));

        let editor_font_family_sub = cx.subscribe(
            &editor_font_combo,
            |_, _, event: &ComboboxEvent<FontChoices>, cx| {
                let ComboboxEvent::Confirm(values) = event else {
                    return;
                };
                let Some(family) = values.first().cloned() else {
                    return;
                };
                let store = cx.global::<SettingsStore>().0.clone();
                store.update(cx, |state, cx| state.set_editor_font_family(family, cx));
            },
        );
        let tile_font_family_sub = cx.subscribe(
            &tile_font_combo,
            |_, _, event: &ComboboxEvent<FontChoices>, cx| {
                let ComboboxEvent::Confirm(values) = event else {
                    return;
                };
                let Some(family) = values.first().cloned() else {
                    return;
                };
                let store = cx.global::<SettingsStore>().0.clone();
                store.update(cx, |state, cx| state.set_tile_font_family(family, cx));
            },
        );
        // 拖动中只刷新本页示例；松开后落库并广播到编辑器 / 磁贴
        let editor_font_sub = cx.subscribe(
            &editor_font_slider,
            |page, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    page.typography.editor_font_preview = value.end();
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
                |page, _, event: &SliderEvent, cx| match event {
                    SliderEvent::Change(value) => {
                        page.typography.tile_font_preview = value.end();
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
            |page, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    page.typography.editor_line_preview = value.end();
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
                |page, _, event: &SliderEvent, cx| match event {
                    SliderEvent::Change(value) => {
                        page.typography.tile_line_preview = value.end();
                        cx.notify();
                    }
                    SliderEvent::Release(value) => {
                        let store = cx.global::<SettingsStore>().0.clone();
                        store.update(cx, |state, cx| state.set_tile_line_height(value.end(), cx));
                    }
                },
            );

        let snapshot = FontSnapshot {
            custom,
            editor: editor_family,
            tile: tile_family,
        };
        Self {
            editor_font_slider,
            editor_font_preview: editor_px,
            editor_font_combo,
            editor_line_slider,
            editor_line_preview: editor_line,
            tile_font_slider,
            tile_font_preview: tile_px,
            tile_font_combo,
            tile_line_slider,
            tile_line_preview: tile_line,
            snapshot,
            font_picker: FontPickerHost::default(),
            _subscriptions: vec![
                editor_font_family_sub,
                tile_font_family_sub,
                editor_font_sub,
                tile_font_sub,
                editor_line_sub,
                tile_line_sub,
            ],
        }
    }

    /// 设置变化后重建两个字体下拉：自定义字体列表或当前字体变化时，
    /// 用最新候选替换下拉项并按保存值恢复选中项（不触发确认事件）。
    pub(super) fn sync(&mut self, window: &mut Window, cx: &mut Context<SettingsPage>) {
        let (editor_choices, tile_choices, custom, editor, tile) = {
            let state = cx.global::<SettingsStore>().0.read(cx);
            let unchanged = self.snapshot.custom.iter().eq(state.custom_font_families())
                && self.snapshot.editor.as_deref() == state.editor_font_family()
                && self.snapshot.tile.as_deref() == state.tile_font_family();
            if unchanged {
                return;
            }

            let editor = state.editor_font_family().map(str::to_string);
            let tile = state.tile_font_family().map(str::to_string);
            let editor_choices = font_choices::choices(
                state.installed_font_names(),
                state.custom_font_families(),
                editor.as_deref(),
            );
            let tile_choices = font_choices::choices(
                state.installed_font_names(),
                state.custom_font_families(),
                tile.as_deref(),
            );
            (
                editor_choices,
                tile_choices,
                state.custom_font_families().to_vec(),
                editor,
                tile,
            )
        };
        self.snapshot = FontSnapshot {
            custom,
            editor: editor.clone(),
            tile: tile.clone(),
        };
        set_font_choices(&self.editor_font_combo, editor_choices, editor, window, cx);
        set_font_choices(&self.tile_font_combo, tile_choices, tile, window, cx);
    }
}

/// 字体下拉实体：选项为精简候选列表 + 用户添加的系统字体，无需搜索框。
fn font_combo(
    window: &mut Window,
    cx: &mut Context<SettingsPage>,
    choices: &[FontChoice],
    index: usize,
) -> Entity<FontCombobox> {
    let items = FontChoices::new(choices.to_vec());
    cx.new(|cx| ComboboxState::new(items, vec![IndexPath::new(index)], window, cx))
}

/// 替换下拉候选并按字体族恢复选中项（默认项对应 `None`）。
fn set_font_choices(
    combo: &Entity<FontCombobox>,
    choices: Vec<FontChoice>,
    family: Option<String>,
    window: &mut Window,
    cx: &mut Context<SettingsPage>,
) {
    combo.update(cx, |combo, cx| {
        combo.set_items(FontChoices::new(choices), window, cx);
        combo.set_selected_values(&[family], window, cx);
    });
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
