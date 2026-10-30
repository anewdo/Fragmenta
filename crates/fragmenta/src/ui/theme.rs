//! 内置 JSON 主题：资源嵌入、选择应用与开发期热重载。

use std::rc::Rc;
use std::time::Duration;

use fragmenta_core::settings::ThemeMode;
use gpui_kit::App;
use gpui_kit::Global;
use gpui_kit::component::{Theme, ThemeConfig, ThemeMode as KitThemeMode, ThemeRegistry};

use super::tokens;

const LIGHT_NAME: &str = "Fragmenta Light";
const DARK_NAME: &str = "Fragmenta Dark";
const LITERARY_GREEN_NAME: &str = "Fragmenta Literary Green";
const CERAMIC_NAME: &str = "Fragmenta Ceramic";

/// 弹窗入场 / 退场时长（ADR-0010）：kit 两种方向共用一个 token，
/// 取参考退场时长 0.28s，比 kit 默认 180ms 更从容。
const DIALOG_DURATION: Duration = Duration::from_millis(280);

/// 最近一次应用的主题选择：开发期热重载按同一选择重放。
struct AppliedTheme(ThemeMode);

impl Global for AppliedTheme {}

/// 同步登记内嵌主题，首次应用前即可使用，发布时无需外置资源。
pub(crate) fn init(cx: &mut App) {
    let registry = ThemeRegistry::global_mut(cx);
    for (json, label) in [
        (include_str!("../../themes/light.json"), "Light"),
        (include_str!("../../themes/dark.json"), "Dark"),
        (
            include_str!("../../themes/literary_green.json"),
            "Literary Green",
        ),
        (include_str!("../../themes/ceramic.json"), "Ceramic"),
    ] {
        registry
            .load_themes_from_str(json)
            .unwrap_or_else(|err| panic!("Invalid embedded {label} theme: {err}"));
    }
    let light = definition(ThemeMode::Light, KitThemeMode::Light, cx);
    let dark = definition(ThemeMode::Dark, KitThemeMode::Dark, cx);
    Theme::update(cx, |theme| {
        theme.light_theme = light;
        theme.dark_theme = dark;
    });
    cx.set_global(AppliedTheme(ThemeMode::Light));
}

/// 主题选择 → kit 定义名与深浅模式；Auto 按系统外观落在 Light / Dark。
fn resolve(theme: ThemeMode, system: KitThemeMode) -> (&'static str, KitThemeMode) {
    match theme {
        ThemeMode::Light => (LIGHT_NAME, KitThemeMode::Light),
        ThemeMode::Dark => (DARK_NAME, KitThemeMode::Dark),
        ThemeMode::LiteraryGreen => (LITERARY_GREEN_NAME, KitThemeMode::Light),
        ThemeMode::Ceramic => (CERAMIC_NAME, KitThemeMode::Light),
        ThemeMode::Auto => {
            if system.is_dark() {
                (DARK_NAME, KitThemeMode::Dark)
            } else {
                (LIGHT_NAME, KitThemeMode::Light)
            }
        }
    }
}

fn definition(theme: ThemeMode, system: KitThemeMode, cx: &App) -> Rc<ThemeConfig> {
    let (name, mode) = resolve(theme, system);
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
/// Auto 不是具体配色，预览时按浅色解析。
pub(crate) fn preview(theme: ThemeMode, cx: &App) -> Theme {
    let config = definition(theme, KitThemeMode::Light, cx);
    let mut theme = Theme::default();
    theme.apply_config(&config);
    theme
}

/// 统一主题写入入口；界面几何继续遵循尺寸与文字令牌。
pub(crate) fn apply(theme: ThemeMode, system: KitThemeMode, cx: &mut App) {
    let config = definition(theme, system, cx);
    Theme::update(cx, |theme| {
        theme.apply_config(&config);
        theme.radius = tokens::radius::SM;
        theme.radius_lg = tokens::radius::MD;
        // 焦点反馈统一改由 1px 主色描边承担（ADR-0010），关闭 kit 的 3px 焦点环。
        theme.focus_ring = false;
        theme.motion.duration_normal = DIALOG_DURATION;
    });
    cx.set_global(AppliedTheme(theme));
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
            apply(ThemeMode::Light, KitThemeMode::Light, cx);
        });
        let light = cx.update(|cx| {
            let theme = Theme::global(cx);
            assert_eq!(theme.background, rgb(0xFFFFFF).into());
            assert_eq!(theme.primary, rgb(0x0F6CBD).into());
            assert!(!theme.shadow);
            assert_eq!(preview(ThemeMode::Light, cx).colors, theme.colors);
            theme.colors
        });
        cx.update(|cx| apply(ThemeMode::Dark, KitThemeMode::Light, cx));
        cx.update(|cx| {
            let theme = Theme::global(cx);
            assert!(theme.mode.is_dark());
            assert!(theme.shadow);
            assert_ne!(theme.background, light.background);
            assert_eq!(preview(ThemeMode::Dark, cx).colors, theme.colors);
        });
        cx.update(|cx| apply(ThemeMode::Light, KitThemeMode::Light, cx));
        cx.update(|cx| assert_eq!(Theme::global(cx).colors, light));
    }

    /// 色系主题以浅色模式生效，预览色与内嵌 JSON 一致。
    #[gpui_kit::test]
    fn color_themes_apply_as_light_themes(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::component::theme::init(cx);
            init(cx);
        });
        let cases = [
            (ThemeMode::LiteraryGreen, 0xF4F1EA, 0x2E5A44),
            (ThemeMode::Ceramic, 0xFBF9F5, 0xDA7756),
        ];
        for (choice, background, primary) in cases {
            cx.update(|cx| apply(choice, KitThemeMode::Dark, cx));
            cx.update(|cx| {
                let theme = Theme::global(cx);
                assert!(!theme.mode.is_dark());
                assert_eq!(theme.background, rgb(background).into());
                assert_eq!(theme.primary, rgb(primary).into());
                let preview = preview(choice, cx);
                assert_eq!(preview.colors, theme.colors);
            });
        }
    }
}

/// 开发构建监听源码目录；发布构建只使用内嵌主题。
#[cfg(debug_assertions)]
pub(crate) fn watch(cx: &mut App) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("themes");
    cx.observe_global::<ThemeRegistry>(|cx| {
        // 等待 kit 完成注册表更新，再按当前选择重放（Auto 沿用已解析的深浅模式）。
        cx.defer(|cx| {
            let choice = cx.global::<AppliedTheme>().0;
            let system = Theme::global(cx).mode;
            apply(choice, system, cx);
        });
    })
    .detach();
    if let Err(err) = ThemeRegistry::watch_dir(dir, cx, |_| {}) {
        eprintln!("[fragmenta] 主题热重载启动失败: {err}");
    }
}
