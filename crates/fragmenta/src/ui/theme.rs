//! 内置 JSON 主题：资源嵌入、模式应用与开发期热重载。

use std::rc::Rc;

use gpui_kit::App;
use gpui_kit::component::{Theme, ThemeConfig, ThemeMode, ThemeRegistry};

use super::tokens;

const LIGHT_NAME: &str = "Fragmenta Light";
const DARK_NAME: &str = "Fragmenta Dark";

/// 同步登记内嵌主题，首次应用前即可使用，发布时无需外置资源。
pub(crate) fn init(cx: &mut App) {
    let registry = ThemeRegistry::global_mut(cx);
    registry
        .load_themes_from_str(include_str!("../../themes/light.json"))
        .expect("Invalid embedded Light theme");
    registry
        .load_themes_from_str(include_str!("../../themes/dark.json"))
        .expect("Invalid embedded Dark theme");
    let light = definition(ThemeMode::Light, cx);
    let dark = definition(ThemeMode::Dark, cx);
    Theme::update(cx, |theme| {
        theme.light_theme = light;
        theme.dark_theme = dark;
    });
}

fn definition(mode: ThemeMode, cx: &App) -> Rc<ThemeConfig> {
    let name = if mode.is_dark() {
        DARK_NAME
    } else {
        LIGHT_NAME
    };
    ThemeRegistry::global(cx)
        .themes()
        .get(name)
        .cloned()
        // 开发期保存中的文件可能短暂无效，沿用最近一次成功应用的定义。
        .unwrap_or_else(|| {
            let theme = Theme::global(cx);
            if mode.is_dark() {
                theme.dark_theme.clone()
            } else {
                theme.light_theme.clone()
            }
        })
}

/// 解析预览与实际窗口共用的主题定义，不修改当前主题。
pub(crate) fn preview(mode: ThemeMode, cx: &App) -> Theme {
    let mut theme = Theme::default();
    theme.apply_config(&definition(mode, cx));
    theme
}

/// 统一主题写入入口；界面几何继续遵循尺寸与文字令牌。
pub(crate) fn apply(mode: ThemeMode, cx: &mut App) {
    let config = definition(mode, cx);
    Theme::update(cx, |theme| {
        theme.apply_config(&config);
        theme.radius = tokens::radius::SM;
        theme.radius_lg = tokens::radius::MD;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{TestAppContext, rgb};

    /// 深浅色往返后恢复完整配色与阴影，预览读取相同定义。
    #[gpui_kit::test]
    fn embedded_themes_and_preview_survive_mode_roundtrip(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::component::theme::init(cx);
            init(cx);
            apply(ThemeMode::Light, cx);
        });
        let light = cx.update(|cx| {
            let theme = Theme::global(cx);
            assert_eq!(theme.background, rgb(0xFFFFFF).into());
            assert_eq!(theme.primary, rgb(0x0F6CBD).into());
            assert!(!theme.shadow);
            assert_eq!(preview(ThemeMode::Light, cx).colors, theme.colors);
            theme.colors
        });
        cx.update(|cx| apply(ThemeMode::Dark, cx));
        cx.update(|cx| {
            let theme = Theme::global(cx);
            assert!(theme.mode.is_dark());
            assert!(theme.shadow);
            assert_ne!(theme.background, light.background);
            assert_eq!(preview(ThemeMode::Dark, cx).colors, theme.colors);
        });
        cx.update(|cx| apply(ThemeMode::Light, cx));
        cx.update(|cx| assert_eq!(Theme::global(cx).colors, light));
    }
}

/// 开发构建监听源码目录；发布构建只使用内嵌主题。
#[cfg(debug_assertions)]
pub(crate) fn watch(cx: &mut App) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("themes");
    cx.observe_global::<ThemeRegistry>(|cx| {
        // 等待 kit 完成注册表更新，再统一应用尺寸规范。
        cx.defer(|cx| apply(Theme::global(cx).mode, cx));
    })
    .detach();
    if let Err(err) = ThemeRegistry::watch_dir(dir, cx, |_| {}) {
        eprintln!("[fragmenta] 主题热重载启动失败: {err}");
    }
}
