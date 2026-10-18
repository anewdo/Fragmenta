//! 设置状态（架构 §5.6 / D8 副作用集中）：运行期 `Settings` 的唯一权威副本，
//! 修改即持久化（settings.json，ADR-0004 锚点）。
//!
//! 主题模式的解析与应用（含 Auto 系统外观桥接、内置色系覆盖）收敛于此：
//! 构造时应用一次（启动期唯一入口），此后每次修改即时生效于全部存活窗口
//! （`Theme::change` / `Theme::update` 内部 `refresh_windows`，含已开磁贴）。
//!
//! 字号档位不写全局主题：kit 组件几何按 16px rem 基准设计，全局放大
//! 会使 px 固定几何组件（日历等）文字溢出。字号只作用于内容阅读面，
//! 由各内容组件经 [`SettingsState::content_font_px`] 显式应用。

use std::path::PathBuf;

use fragmenta_core::settings::{FontScale, Settings, ThemeMode};
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode as KitThemeMode};
use gpui_kit::{Context, EventEmitter, Pixels, Subscription, Window, WindowAppearance, px};

use crate::app::SettingsStore;
use crate::theme_schemes::{self, ThemeScheme};

/// 设置变更广播：订阅方（设置页等）据此刷新展示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEvent {
    Changed,
}

impl EventEmitter<SettingsEvent> for SettingsState {}

/// 设置状态：持有当前 `Settings` 与 settings.json 路径。
pub struct SettingsState {
    settings: Settings,
    path: PathBuf,
}

impl SettingsState {
    /// 构造即按当前设置应用主题模式（启动期主题应用唯一入口）。
    pub fn new(settings: Settings, path: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut state = Self { settings, path };
        state.apply_theme_mode(cx);
        state
    }

    /// 当前主题模式。
    pub fn theme(&self) -> ThemeMode {
        self.settings.theme
    }

    /// 当前字号档位。
    pub fn font_scale(&self) -> FontScale {
        self.settings.font_scale
    }

    /// 内容阅读面（笔记列表 / 编辑器与预览 / 待办清单 / 磁贴标题与正文）
    /// 的正文字号：内容组件在容器上 `.text_size()` 应用，文字沿元素树继承。
    pub fn content_font_px(&self) -> Pixels {
        match self.settings.font_scale {
            FontScale::Small => px(14.),
            FontScale::Medium => px(16.),
            FontScale::Large => px(18.),
        }
    }

    /// 关闭主窗口时直接退出；默认 false（托盘驻留）。
    pub fn close_to_quit(&self) -> bool {
        self.settings.close_to_quit
    }

    /// 更新主题模式并立即生效于全部存活窗口（含已开磁贴），持久化 + 广播。
    pub fn set_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        if self.settings.theme == mode {
            return;
        }
        self.settings.theme = mode;
        self.apply_theme_mode(cx);
        self.persist();
        cx.emit(SettingsEvent::Changed);
    }

    /// 更新字号档位：持久化 + 广播（订阅的内容组件据此重渲染生效）。
    pub fn set_font_scale(&mut self, scale: FontScale, cx: &mut Context<Self>) {
        if self.settings.font_scale == scale {
            return;
        }
        self.settings.font_scale = scale;
        self.persist();
        cx.emit(SettingsEvent::Changed);
    }

    /// 更新"关闭主窗口时直接退出"并立即持久化。
    pub fn set_close_to_quit(&mut self, value: bool, cx: &mut Context<Self>) {
        if self.settings.close_to_quit == value {
            return;
        }
        self.settings.close_to_quit = value;
        self.persist();
        cx.emit(SettingsEvent::Changed);
    }

    /// 窗口创建时登记 Auto 桥接（主窗口与磁贴各自调用）：
    /// 先按窗口当前外观校准一次（覆盖无窗口期间系统深浅色变化），
    /// 返回的订阅由窗口视图持有，视图释放时一并取消监听。
    pub fn attach_appearance_bridge(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        self.sync_auto_mode(window.appearance(), cx);
        window.observe_window_appearance(|window, cx| {
            let store = cx.global::<SettingsStore>().0.clone();
            store.update(cx, |state, cx| {
                state.sync_auto_mode(window.appearance(), cx);
            });
        })
    }

    /// Auto 桥接：主题模式为 Auto 时跟随系统外观切换 kit 主题；
    /// 其余模式不受系统变化影响。已处于目标模式时跳过（多窗口监听去重）。
    fn sync_auto_mode(&mut self, appearance: WindowAppearance, cx: &mut Context<Self>) {
        if self.settings.theme != ThemeMode::Auto {
            return;
        }
        let mode = kit_mode_of(appearance);
        if cx.theme().mode != mode {
            Theme::change(mode, None, cx);
        }
    }

    /// 按设置应用主题模式：内置色系 = Light 基线 + 核心 token 覆盖
    /// （ADR-0005）；Auto 解析为当前系统外观（App 级，无窗口期同样可解析）。
    fn apply_theme_mode(&mut self, cx: &mut Context<Self>) {
        match self.settings.theme {
            ThemeMode::Light => Theme::change(KitThemeMode::Light, None, cx),
            ThemeMode::Dark => Theme::change(KitThemeMode::Dark, None, cx),
            ThemeMode::Auto => {
                Theme::change(kit_mode_of(cx.window_appearance()), None, cx)
            }
            ThemeMode::DouShaGreen => {
                Self::apply_color_scheme(theme_schemes::dou_sha_green(), cx)
            }
            ThemeMode::SkyBlue => Self::apply_color_scheme(theme_schemes::sky_blue(), cx),
        }
    }

    /// 内置色系落色：先切 Light 基线（复位 Dark / 上一套色系的残余色），
    /// 再以 `Theme::update` 覆盖核心 token（内部 refresh_windows，全窗口
    /// 即时生效）。切回 Light / Dark / Auto 时 `Theme::change` 重载注册
    /// 主题，覆盖自然清除，无状态残留。
    fn apply_color_scheme(scheme: ThemeScheme, cx: &mut Context<Self>) {
        Theme::change(KitThemeMode::Light, None, cx);
        Theme::update(cx, |theme| scheme.apply(&mut theme.colors));
    }

    /// 持久化到 settings.json；失败不中断交互（eprintln 容错，与项目现状一致）。
    fn persist(&self) {
        if let Err(err) = self.settings.save(&self.path) {
            eprintln!("[fragmenta] 保存设置失败: {err}");
        }
    }
}

/// 系统外观 → kit 主题模式（Auto 解析规则，spike ③ 结论）。
fn kit_mode_of(appearance: WindowAppearance) -> KitThemeMode {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => KitThemeMode::Dark,
        _ => KitThemeMode::Light,
    }
}
