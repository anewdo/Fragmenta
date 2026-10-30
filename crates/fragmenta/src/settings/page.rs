use std::path::PathBuf;

use fragmenta_core::export::{ExportFailure, ExportReport};
use fragmenta_core::model::NoteFilter;
use fragmenta_core::settings::{FONT_SIZE_MAX_PX, FONT_SIZE_MIN_PX, ThemeMode};
use gpui_kit::component::button::Button;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::editor::ImageStore;
use crate::hotkey::HotkeyError;
use crate::notes::NotesStore;
use crate::settings::SettingsStore;
use crate::ui::layout::{
    CardStyle, LayoutClass, content_column, page_scroll, section_title, setting_card,
};
use crate::ui::theme;
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};
use crate::widgets::toast::{Toast, ToastStore};

/// 热键录入框宽度与主题色卡尺寸：设置页局部控件特例。
const HOTKEY_RECORDER_WIDTH: Pixels = px(240.);
const THEME_SWATCH_WIDTH: Pixels = px(88.);
const THEME_SWATCH_HEIGHT: Pixels = px(52.);
/// 字号滑杆宽度：范围仅十余档，短轨让每档的拖动距离足够可感。
const FONT_SLIDER_WIDTH: Pixels = px(220.);
/// 字号滑杆下方的实时示例词：中文 / 英文各一。
const EDITOR_FONT_SAMPLE: (&str, &str) = ("笔记", "Note");
const TILE_FONT_SAMPLE: (&str, &str) = ("速记", "Memo");

/// 设置页视图：常驻于主窗口 shell，订阅设置广播刷新展示。
pub(crate) struct SettingsPage {
    _subscription: Subscription,
    /// 窗口尺寸变化：档位留白随视口宽度切换。
    _bounds_sub: Subscription,
    /// 编辑器字号滑杆与拖动中的实时预览值。
    editor_font_slider: Entity<SliderState>,
    editor_font_preview: f32,
    _editor_font_sub: Subscription,
    /// 小窗字号滑杆与拖动中的实时预览值。
    tile_font_slider: Entity<SliderState>,
    tile_font_preview: f32,
    _tile_font_sub: Subscription,
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
        let (editor_px, tile_px) = {
            let state = settings.read(cx);
            (
                state.editor_font_px().as_f32(),
                state.tile_font_px().as_f32(),
            )
        };
        let editor_font_slider = cx.new(|_| font_slider(editor_px));
        let tile_font_slider = cx.new(|_| font_slider(tile_px));
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
        Self {
            _subscription: subscription,
            _bounds_sub: bounds_sub,
            editor_font_slider,
            editor_font_preview: editor_px,
            _editor_font_sub: editor_font_sub,
            tile_font_slider,
            tile_font_preview: tile_px,
            _tile_font_sub: tile_font_sub,
            hotkey_focus: cx.focus_handle(),
            hotkey_listening: false,
            hotkey_pending_mods: None,
            hotkey_error: None,
            data_dir_error: None,
            autostart_error: None,
        }
    }

    /// "快捷键"组：磁贴呼出热键捕获式录入框与红字错误提示。
    fn hotkey_group(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let hotkey = cx
            .global::<SettingsStore>()
            .0
            .read(cx)
            .tile_hotkey()
            .to_string();
        let card = CardStyle::from_theme(cx.theme());
        let (muted, primary, border, danger) = {
            let colors = cx.theme();
            (
                colors.muted_foreground,
                colors.primary,
                colors.border,
                colors.danger,
            )
        };
        let focus = self.hotkey_focus.clone();
        let listening = self.hotkey_listening;
        let pending_mods = self.hotkey_pending_mods.clone();
        let error = self.hotkey_error;

        let recorder = div()
            .id("hotkey-recorder")
            .track_focus(&focus)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .w(HOTKEY_RECORDER_WIDTH)
            .h(tokens::heights::CONTROL)
            .rounded(tokens::radius::SM)
            .border_1()
            .border_color(if listening { primary } else { border })
            .child(match (listening, pending_mods.as_deref()) {
                (true, Some(mods)) => div()
                    .text_role(TextRole::ControlLabel)
                    .text_color(muted)
                    .child(format!("{mods} + …")),
                (true, None) => div()
                    .text_role(TextRole::ControlLabel)
                    .text_color(muted)
                    .child("按下新组合键…"),
                (false, _) => div()
                    .text_role(TextRole::ControlLabel)
                    .child(pretty_hotkey(&hotkey)),
            })
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                // 点击框外退出监听态，不误改热键
                if this.hotkey_listening {
                    this.hotkey_listening = false;
                    this.hotkey_pending_mods = None;
                    this.hotkey_error = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                // 点击录入框：聚焦并进入监听态
                window.focus(&this.hotkey_focus, cx);
                this.hotkey_listening = true;
                this.hotkey_pending_mods = None;
                this.hotkey_error = None;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if !this.hotkey_listening {
                    return;
                }
                // 监听态吞掉全部按键，避免触发其他快捷键
                window.prevent_default();
                cx.stop_propagation();
                let stroke = &event.keystroke;
                match stroke.key.as_str() {
                    // Esc 退出监听态，不误改热键
                    "escape" => {
                        this.hotkey_listening = false;
                        this.hotkey_pending_mods = None;
                        this.hotkey_error = None;
                    }
                    // 修饰键自身按下：和弦进行中，展示由 on_modifiers_changed 维护
                    "shift" | "control" | "alt" | "platform" | "meta" | "super" | "cmd" | "win"
                    | "function" | "fn" => {}
                    _ => {
                        this.hotkey_pending_mods = None;
                        let candidate = hotkey_candidate(stroke);
                        let store = cx.global::<SettingsStore>().0.clone();
                        match store.update(cx, |state, cx| state.set_hotkey(&candidate, cx)) {
                            Ok(()) => {
                                this.hotkey_listening = false;
                                this.hotkey_error = None;
                            }
                            Err(HotkeyError::Parse(_)) => {
                                this.hotkey_error = Some("无效组合，请按其他组合键");
                            }
                            Err(HotkeyError::Register(_)) => {
                                this.hotkey_listening = false;
                                this.hotkey_error = Some("组合键被其他程序占用，已保留原热键");
                            }
                        }
                    }
                }
                cx.notify();
            }))
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, cx| {
                if !this.hotkey_listening {
                    return;
                }
                if event.modifiers.number_of_modifiers() == 0 {
                    // 和弦中松开全部修饰键而未按非修饰键：纯修饰键组合
                    if this.hotkey_pending_mods.take().is_some() {
                        this.hotkey_error = Some("纯修饰键组合无效，请按下一个非修饰键");
                        cx.notify();
                    }
                } else {
                    let mods = pretty_modifiers(&event.modifiers);
                    if this.hotkey_pending_mods.as_deref() != Some(mods.as_str()) {
                        this.hotkey_pending_mods = Some(mods);
                        cx.notify();
                    }
                }
            }));

        v_flex()
            .w_full()
            .gap(tokens::spacing::SM)
            .child(setting_card(
                card,
                setting_row(
                    "磁贴呼出热键",
                    "点击录入框后直接按下新组合键完成改绑；Esc 或点击框外取消",
                    muted,
                    recorder,
                ),
            ))
            .when_some(error, |el, err| {
                el.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .child(error_text(err, danger)),
                )
            })
    }
    /// "数据"组（ticket 06）：三行布局——当前目录（可打开）／更改存储路径
    /// （目录选择器选完即迁移）／重置为默认位置；迁移错误红字展示。
    fn data_group(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let card = CardStyle::from_theme(cx.theme());
        let (muted, danger) = {
            let colors = cx.theme();
            (colors.muted_foreground, colors.danger)
        };
        let current = cx
            .global::<SettingsStore>()
            .0
            .read(cx)
            .data_dir()
            .display()
            .to_string();
        let error = self.data_dir_error.clone();

        v_flex()
            .w_full()
            .gap(tokens::spacing::SM)
            // 行 1：当前数据存储目录 + 打开
            .child(setting_card(
                card,
                v_flex()
                    .w_full()
                    .gap(tokens::spacing::XS)
                    .child(
                        h_flex()
                            .w_full()
                            .gap(tokens::spacing::SM)
                            .child(div().flex_1().child("当前数据存储目录"))
                            .child(Button::new("data-dir-open").label("打开当前目录").on_click(
                                |_, _, cx| {
                                    let dir = cx
                                        .global::<SettingsStore>()
                                        .0
                                        .read(cx)
                                        .data_dir()
                                        .to_path_buf();
                                    crate::win32::open_folder(&dir);
                                },
                            )),
                    )
                    .child(
                        div()
                            .text_role(TextRole::Meta)
                            .text_color(muted)
                            .child(current),
                    ),
            ))
            // 行 2：更改存储路径（目录选择器选完即迁移）
            .child(setting_card(
                card,
                h_flex()
                    .w_full()
                    .gap(tokens::spacing::SM)
                    .child(div().flex_1().child("更改存储路径"))
                    .child(Button::new("data-dir-change").label("选择新位置").on_click(
                        cx.listener(|this, _, window, cx| {
                            // 系统目录选择器（COM IFileOpenDialog）；取消则不迁移
                            let owner = crate::win32::window_hwnd(window);
                            let Some(picked) = crate::win32::pick_folder(owner) else {
                                return;
                            };
                            let store = cx.global::<SettingsStore>().0.clone();
                            match store.update(cx, |state, cx| state.set_data_dir(picked, cx)) {
                                Ok(()) => this.data_dir_error = None,
                                Err(err) => this.data_dir_error = Some(err.to_string()),
                            }
                            cx.notify();
                        }),
                    )),
            ))
            // 行 3：重置为默认位置（exe 旁 data/）
            .child(setting_card(
                card,
                v_flex()
                    .w_full()
                    .gap(tokens::spacing::XS)
                    .child(
                        h_flex()
                            .w_full()
                            .gap(tokens::spacing::SM)
                            .child(div().flex_1().child("重置为默认位置"))
                            .child(Button::new("data-dir-reset").label("重置").on_click(
                                cx.listener(|this, _, _, cx| {
                                    // 默认位置与启动逻辑同源（{exe_dir}/data）
                                    let target =
                                        fragmenta_core::settings::Settings::default().data_dir;
                                    let store = cx.global::<SettingsStore>().0.clone();
                                    let already_default =
                                        store.read_with(cx, |state, _| state.data_dir() == target);
                                    if already_default {
                                        return;
                                    }
                                    match store
                                        .update(cx, |state, cx| state.set_data_dir(target, cx))
                                    {
                                        Ok(()) => this.data_dir_error = None,
                                        Err(err) => this.data_dir_error = Some(err.to_string()),
                                    }
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .text_role(TextRole::Meta)
                            .text_color(muted)
                            .child("将数据存储位置重置为 exe 所在目录"),
                    ),
            ))
            // 行 4：笔记导出为 md（ticket 07）——目录选择器选目录，后台导出，toast 报告
            .child(setting_card(
                card,
                h_flex()
                    .w_full()
                    .gap(tokens::spacing::SM)
                    .child(div().flex_1().child("笔记导出为 md"))
                    .child(
                        Button::new("data-export-md")
                            .label("导出")
                            .on_click(cx.listener(|_, _, window, cx| {
                                // 系统目录选择器（COM IFileOpenDialog）；取消则不导出
                                let owner = crate::win32::window_hwnd(window);
                                let Some(out_dir) = crate::win32::pick_folder(owner) else {
                                    return;
                                };
                                run_export(out_dir, cx);
                            })),
                    ),
            ))
            .when_some(error, |el, err| {
                el.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .child(error_text(err, danger)),
                )
            })
    }

    /// "系统"组（ticket 08 / 09）：关闭主窗口语义开关、开机自启动开关
    /// （注册表操作失败红字展示，开关状态不前移）。
    fn system_group(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<SettingsStore>().0.read(cx);
        let (close_to_quit, autostart) = (settings.close_to_quit(), settings.autostart());
        let card = CardStyle::from_theme(cx.theme());
        let (muted, danger) = {
            let colors = cx.theme();
            (colors.muted_foreground, colors.danger)
        };
        let error = self.autostart_error.clone();

        v_flex()
            .w_full()
            .gap(tokens::spacing::SM)
            .child(setting_card(
                card,
                setting_row(
                    "关闭主窗口时直接退出",
                    "默认关闭：关闭主窗口后驻留系统托盘，热键呼出磁贴不受影响",
                    muted,
                    Switch::new("close-to-quit")
                        .checked(close_to_quit)
                        .on_click(|checked: &bool, _window, cx| {
                            let settings = cx.global::<SettingsStore>().0.clone();
                            settings.update(cx, |state, cx| state.set_close_to_quit(*checked, cx));
                        }),
                ),
            ))
            .child(setting_card(
                card,
                setting_row(
                    "开机自启动",
                    "登录 Windows 后静默启动并驻留托盘，不打开主窗口",
                    muted,
                    Switch::new("autostart")
                        .checked(autostart)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            let store = cx.global::<SettingsStore>().0.clone();
                            match store.update(cx, |state, cx| state.set_autostart(*checked, cx)) {
                                Ok(()) => this.autostart_error = None,
                                Err(err) => this.autostart_error = Some(err.to_string()),
                            }
                            cx.notify();
                        })),
                ),
            ))
            .when_some(error, |el, err| {
                el.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .child(error_text(err, danger)),
                )
            })
    }
}

/// 执行导出（ticket 07）：待写草稿先落库并直查全量笔记，导出在后台线程
/// 进行（应用可用不阻塞），完成后以 toast 报告（笔记数 / 资产数 / 失败项）。
fn run_export(out_dir: PathBuf, cx: &mut Context<SettingsPage>) {
    let notes_store = cx.global::<NotesStore>().0.clone();
    // 导出内容与界面一致：先落库全部待写草稿（失败槽保留重试，不影响导出）
    let _ = notes_store.update(cx, |state, cx| state.flush_all(cx));
    let query = notes_store.update(cx, |state, _| state.notes(&NoteFilter::default()));
    let images = cx.global::<ImageStore>().0.clone();
    let toast = cx.global::<ToastStore>().0.clone();
    match query {
        Ok(notes) => {
            // 导出在后台线程执行；完成后回主线程 toast 报告
            let export = cx.background_spawn(async move {
                fragmenta_core::export::export_notes(&notes, &out_dir, &images)
            });
            cx.spawn(async move |_, cx| {
                let report = export.await;
                let _ = toast.update(cx, |state, cx| {
                    state.show(export_result_toast(report), cx);
                });
            })
            .detach();
        }
        Err(err) => {
            toast.update(cx, |state, cx| {
                state.show(
                    Toast {
                        title: "导出失败".to_string(),
                        detail: Some(format!("读取笔记列表失败：{err}")),
                        failure: true,
                    },
                    cx,
                );
            });
        }
    }
}

/// 导出结果 → toast：成功项计数入标题，失败项入明细（最多列 3 条，余下计数）。
fn export_result_toast(result: fragmenta_core::export::Result<ExportReport>) -> Toast {
    match result {
        Ok(report) => {
            let failures = report.failures.len();
            let title = if failures == 0 {
                format!("已导出 {} 篇笔记、{} 张图片", report.notes, report.assets)
            } else {
                format!(
                    "已导出 {} 篇笔记、{} 张图片，{} 项失败",
                    report.notes, report.assets, failures
                )
            };
            let detail = (failures > 0).then(|| {
                let mut lines: Vec<String> = report
                    .failures
                    .iter()
                    .take(3)
                    .map(|failure| match failure {
                        ExportFailure::Note { title, error, .. } => {
                            format!("《{title}》：{error}")
                        }
                        ExportFailure::Asset { reference, error } => {
                            format!("{reference}：{error}")
                        }
                    })
                    .collect();
                let rest = failures - lines.len();
                if rest > 0 {
                    lines.push(format!("……另有 {rest} 项失败"));
                }
                lines.join("\n")
            });
            Toast {
                title,
                detail,
                failure: failures > 0,
            }
        }
        // 批次级失败（输出目录不可建立 / 不可写）：尚未写出任何文件，无半成品
        Err(err) => Toast {
            title: "导出失败".to_string(),
            detail: Some(format!("无法写入目标目录：{err}")),
            failure: true,
        },
    }
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<SettingsStore>().0.read(cx).theme();
        let (muted, primary, border, foreground) = {
            let theme_colors = cx.theme();
            (
                theme_colors.muted_foreground,
                theme_colors.primary,
                theme_colors.border,
                theme_colors.foreground,
            )
        };
        let card = CardStyle::from_theme(cx.theme());
        let content = content_column(tokens::widths::SETTINGS_CONTENT)
            .gap(tokens::spacing::XXL)
            .child(setting_section(
                "外观",
                foreground,
                appearance_group(
                    theme,
                    self.editor_font_preview,
                    self.tile_font_preview,
                    &self.editor_font_slider,
                    &self.tile_font_slider,
                    muted,
                    primary,
                    border,
                    foreground,
                    card,
                    cx,
                ),
            ))
            .child(setting_section("快捷键", foreground, self.hotkey_group(cx)))
            .child(setting_section("数据", foreground, self.data_group(cx)))
            .child(setting_section("系统", foreground, self.system_group(cx)));
        page_scroll(
            LayoutClass::from_width(window.viewport_size().width),
            content,
        )
    }
}

/// 设置分组：区块标题 + 下方子项卡片列表，组与组之间由外层更大间距分隔。
fn setting_section(
    title: &'static str,
    foreground: Hsla,
    items: impl IntoElement,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(tokens::spacing::MD)
        .child(section_title(title, foreground))
        .child(items)
}

/// 设置项错误提示：固定元信息字号 + 危险色，右对齐展示。
fn error_text(message: impl IntoElement, danger: Hsla) -> impl IntoElement {
    div()
        .text_role(TextRole::Meta)
        .text_color(danger)
        .child(message)
}

/// "外观"组：主题色卡单选 + 编辑器字号 / 小窗字号两个像素滑杆。
#[allow(clippy::too_many_arguments)]
fn appearance_group(
    theme: ThemeMode,
    editor_font_preview: f32,
    tile_font_preview: f32,
    editor_slider: &Entity<SliderState>,
    tile_slider: &Entity<SliderState>,
    muted: Hsla,
    primary: Hsla,
    border: Hsla,
    foreground: Hsla,
    card: CardStyle,
    cx: &App,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(tokens::spacing::SM)
        .child(setting_card(
            card,
            theme_section(theme, muted, primary, border, cx),
        ))
        .child(font_size_card(
            "编辑器字号调整",
            "调整主窗口编辑器与列表的正文大小；拖动滑杆即时预览",
            editor_font_preview,
            EDITOR_FONT_SAMPLE,
            editor_slider,
            muted,
            foreground,
            card,
        ))
        .child(font_size_card(
            "小窗字号",
            "调整磁贴小窗标题与正文的大小；拖动滑杆即时预览",
            tile_font_preview,
            TILE_FONT_SAMPLE,
            tile_slider,
            muted,
            foreground,
            card,
        ))
}

/// 字号设置卡：标题 + 当前像素值、滑杆、滑杆下方的中英文示例。
#[allow(clippy::too_many_arguments)]
fn font_size_card(
    title: &'static str,
    description: &'static str,
    value: f32,
    samples: (&'static str, &'static str),
    slider: &Entity<SliderState>,
    muted: Hsla,
    foreground: Hsla,
    card: CardStyle,
) -> impl IntoElement {
    setting_card(
        card,
        v_flex()
            .w_full()
            .gap(tokens::spacing::MD)
            .child(setting_row(
                title,
                description,
                muted,
                div()
                    .text_role(TextRole::ControlLabel)
                    .text_color(muted)
                    .child(format!("{value:.0} px")),
            ))
            .child(Slider::new(slider).w(FONT_SLIDER_WIDTH))
            .child(
                h_flex()
                    .w_full()
                    .items_baseline()
                    .gap(tokens::spacing::XL)
                    .child(
                        div()
                            .text_role(TextRole::Meta)
                            .text_color(muted)
                            .child("示例"),
                    )
                    .child(font_sample(samples.0, value, foreground))
                    .child(font_sample(samples.1, value, foreground)),
            ),
    )
}

/// 实时字号示例词：按当前滑杆值渲染。
fn font_sample(word: &'static str, size: f32, color: Hsla) -> impl IntoElement {
    div().text_size(px(size)).text_color(color).child(word)
}

/// 字号滑杆：整数步进，范围由 core 设置常量统一约束。
fn font_slider(value: f32) -> SliderState {
    SliderState::new()
        .min(FONT_SIZE_MIN_PX)
        .max(FONT_SIZE_MAX_PX)
        .step(1.)
        .default_value(value.clamp(FONT_SIZE_MIN_PX, FONT_SIZE_MAX_PX))
}

/// 主题色卡预览的三区域颜色：菜单 / 标题栏 / 内容。
struct MockColors {
    chrome: Hsla,
    title: Hsla,
    content: Hsla,
}

/// 单套配色的矩形预览块：左条 = 侧栏、上条 = 标题栏、其余 = 内容。
fn mock_preview(colors: MockColors, chrome_w: Pixels, title_h: Pixels) -> Div {
    h_flex()
        .size_full()
        .child(div().w(chrome_w).h_full().flex_shrink_0().bg(colors.chrome))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(div().h(title_h).flex_shrink_0().bg(colors.title))
                .child(div().flex_1().bg(colors.content)),
        )
}

/// 缩略图读取实际主题定义，与窗口及开发重载保持一致。
fn theme_mock(theme: ThemeMode, cx: &App) -> MockColors {
    let theme = theme::preview(theme, cx);
    MockColors {
        chrome: theme.sidebar,
        title: theme.title_bar,
        content: theme.background,
    }
}

/// 主题节：说明文字 + 矩形颜色按钮行（选中即整套配色生效，枚举单选）。
fn theme_section(
    theme: ThemeMode,
    muted: Hsla,
    primary: Hsla,
    border: Hsla,
    cx: &App,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(tokens::spacing::SM)
        .child(
            v_flex()
                .gap(tokens::spacing::XS)
                .child(div().text_role(TextRole::ControlLabel).child("主题"))
                .child(
                    div()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .child("切换即时生效于全部窗口；自动仅在浅色 / 深色之间跟随系统"),
                ),
        )
        .child(
            h_flex()
                .gap(tokens::spacing::MD)
                .flex_wrap()
                .child(theme_swatch(
                    "theme-pick-auto",
                    "自动",
                    theme == ThemeMode::Auto,
                    ThemeMode::Auto,
                    // 左半 Light / 右半 Dark：跟随系统深浅色的双色预览
                    h_flex()
                        .size_full()
                        .child(
                            mock_preview(theme_mock(ThemeMode::Light, cx), px(9.), px(10.))
                                .flex_1()
                                .min_w_0(),
                        )
                        .child(
                            mock_preview(theme_mock(ThemeMode::Dark, cx), px(9.), px(10.))
                                .flex_1()
                                .min_w_0(),
                        ),
                    primary,
                    border,
                    muted,
                ))
                .child(theme_swatch(
                    "theme-pick-light",
                    "浅色",
                    theme == ThemeMode::Light,
                    ThemeMode::Light,
                    mock_preview(theme_mock(ThemeMode::Light, cx), px(18.), px(12.)),
                    primary,
                    border,
                    muted,
                ))
                .child(theme_swatch(
                    "theme-pick-dark",
                    "深色",
                    theme == ThemeMode::Dark,
                    ThemeMode::Dark,
                    mock_preview(theme_mock(ThemeMode::Dark, cx), px(18.), px(12.)),
                    primary,
                    border,
                    muted,
                ))
                .child(theme_swatch(
                    "theme-pick-literary-green",
                    "绿色",
                    theme == ThemeMode::LiteraryGreen,
                    ThemeMode::LiteraryGreen,
                    mock_preview(theme_mock(ThemeMode::LiteraryGreen, cx), px(18.), px(12.)),
                    primary,
                    border,
                    muted,
                ))
                .child(theme_swatch(
                    "theme-pick-ceramic",
                    "橙色",
                    theme == ThemeMode::Ceramic,
                    ThemeMode::Ceramic,
                    mock_preview(theme_mock(ThemeMode::Ceramic, cx), px(18.), px(12.)),
                    primary,
                    border,
                    muted,
                )),
        )
}

/// 矩形颜色按钮：窗口形预览块 + 下方标签；选中描主色边，点击整套配色生效。
fn theme_swatch(
    id: &'static str,
    label: &'static str,
    selected: bool,
    mode: ThemeMode,
    preview: impl IntoElement,
    primary: Hsla,
    border: Hsla,
    muted: Hsla,
) -> impl IntoElement {
    v_flex()
        .gap(tokens::spacing::XS)
        .child(
            div()
                .id(id)
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let settings = cx.global::<SettingsStore>().0.clone();
                    settings.update(cx, |state, cx| state.set_theme(mode, cx));
                })
                .w(THEME_SWATCH_WIDTH)
                .h(THEME_SWATCH_HEIGHT)
                .rounded(tokens::radius::SM)
                .overflow_hidden()
                .border_2()
                .border_color(if selected { primary } else { border })
                .when(!selected, |el| {
                    el.hover(|style| style.border_color(primary.opacity(0.6)))
                })
                .child(preview),
        )
        .child(
            h_flex().w(THEME_SWATCH_WIDTH).justify_center().child(
                div()
                    .text_role(TextRole::Meta)
                    .text_color(if selected { primary } else { muted })
                    .child(label),
            ),
        )
}

/// 设置行布局：左侧标题 + 说明，右侧控件。
fn setting_row(
    title: &'static str,
    description: &'static str,
    muted: Hsla,
    field: impl IntoElement,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(tokens::spacing::LG)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(tokens::spacing::XS)
                .child(div().text_role(TextRole::ControlLabel).child(title))
                .child(
                    div()
                        .text_role(TextRole::Meta)
                        .text_color(muted)
                        .child(description),
                ),
        )
        .child(field)
}

/// gpui 按键事件 → global_hotkey 解析串（如 "ctrl+alt+n"）。
///
/// 键名统一大写以匹配解析器（"n"→"N"、"f5"→"F5"，符号与数字原样）；
/// 组合序为 shift + ctrl + alt + win + 主键。gpui 对 shift 组合键可能上报
/// 变形字符（如 "<"），此类候选会在解析层被拒并红字提示。
fn hotkey_candidate(stroke: &Keystroke) -> String {
    let mods = &stroke.modifiers;
    let mut candidate = String::new();
    if mods.shift {
        candidate.push_str("shift+");
    }
    if mods.control {
        candidate.push_str("ctrl+");
    }
    if mods.alt {
        candidate.push_str("alt+");
    }
    if mods.platform {
        candidate.push_str("super+");
    }
    candidate.push_str(&stroke.key.to_uppercase());
    candidate
}

/// 修饰键状态 → 展示串（如 "Ctrl + Alt"）。
fn pretty_modifiers(mods: &Modifiers) -> String {
    let mut parts = Vec::new();
    if mods.control {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    if mods.platform {
        parts.push("Win");
    }
    parts.join(" + ")
}

/// 设置串 → 展示串："ctrl+alt+n" → "Ctrl + Alt + N"。
/// 兼容 global_hotkey 规范形式（"KeyN" / "Digit5"）与常见修饰键别名。
fn pretty_hotkey(hotkey: &str) -> String {
    hotkey
        .split('+')
        .map(pretty_token)
        .collect::<Vec<_>>()
        .join(" + ")
}

/// 单个热键 token → 展示形式。
fn pretty_token(token: &str) -> String {
    let trimmed = token.trim();
    let lower = trimmed.to_lowercase();
    match lower.as_str() {
        "ctrl" | "control" => return "Ctrl".to_string(),
        "alt" => return "Alt".to_string(),
        "shift" => return "Shift".to_string(),
        "super" | "cmd" | "win" => return "Win".to_string(),
        _ => {}
    }
    if let Some(letter) = lower.strip_prefix("key").filter(|rest| rest.len() == 1) {
        return letter.to_uppercase();
    }
    if let Some(digit) = lower.strip_prefix("digit").filter(|rest| rest.len() == 1) {
        return digit.to_string();
    }
    // 单字符键大写；命名键（f5 / space 等）首字母大写
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}
