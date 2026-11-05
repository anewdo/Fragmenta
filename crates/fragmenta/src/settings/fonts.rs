//! 阅读字体候选与本机字体匹配。
//!
//! 候选只列文档阅读常用字体；设置里保存的字体族统一经本模块约束到本机
//! 实际安装名。未安装时回落系统默认，避免把不存在的族名交给 GPUI：
//! GPUI 对缺失字体会回退到 Segoe UI，中文再由 DirectWrite 补字形，
//! 造成中英文来源与行高基线不一致。

use std::rc::Rc;

use fragmenta_core::settings::{Settings, normalize_font_family};

/// 应用自定义字体的排版位：编辑器排版或磁贴小窗排版。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontScope {
    Editor,
    Tile,
}

/// 字体设置：启动时的本机字体族快照与两个排版位已解析的字体族。
/// 读取侧（编辑器 / 小窗每帧渲染）直接取缓存，不扫描字体列表。
///
/// 两个排版位的字体族只能经本模块的 `set` / `apply` / `remove_custom`
/// 写入；直接改 `Settings` 对应字段会让缓存与保存值失去同步。
pub(crate) struct FontSettings {
    installed: Rc<Vec<String>>,
    editor: Option<String>,
    tile: Option<String>,
}

impl FontSettings {
    pub(crate) fn load(settings: &Settings, installed: Vec<String>) -> Self {
        let editor = Self::resolve(&installed, settings.editor_font_family.clone());
        let tile = Self::resolve(&installed, settings.tile_font_family.clone());
        Self {
            installed: Rc::new(installed),
            editor,
            tile,
        }
    }

    /// 启动时的系统字体快照（含随附字体），供设置页标注未安装项。
    pub(crate) fn installed(&self) -> &[String] {
        self.installed.as_slice()
    }

    /// 与候选构建、字体选择弹窗共享同一份快照，避免整表复制。
    pub(crate) fn installed_shared(&self) -> Rc<Vec<String>> {
        Rc::clone(&self.installed)
    }

    /// 编辑器字体族（已解析为本机实际安装名）；`None` 跟随系统默认。
    pub(crate) fn editor(&self) -> Option<&str> {
        self.editor.as_deref()
    }

    /// 磁贴小窗字体族（已解析为本机实际安装名）；`None` 跟随系统默认。
    pub(crate) fn tile(&self) -> Option<&str> {
        self.tile.as_deref()
    }

    /// 更新一个排版位：先解析为本机实际安装名（未安装按系统默认）。
    pub(crate) fn set(
        &mut self,
        settings: &mut Settings,
        scope: FontScope,
        family: Option<String>,
    ) -> bool {
        let family = Self::resolve(&self.installed, family);
        self.write(settings, scope, family)
    }

    /// 从系统字体中选择并应用：族名解析后追加到自定义候选（大小写不敏感
    /// 去重），两个字段一次更新。
    pub(crate) fn apply(
        &mut self,
        settings: &mut Settings,
        scope: FontScope,
        family: String,
    ) -> bool {
        let Some(family) = Self::resolve(&self.installed, Some(family)) else {
            return false;
        };
        let mut changed = false;
        if !contains_family(&settings.custom_font_families, &family) {
            settings.custom_font_families.push(family.clone());
            changed = true;
        }
        changed |= self.write(settings, scope, Some(family));
        changed
    }

    /// 从自定义候选移除字体：正在使用该字体的排版位回到系统默认，
    /// 避免移除后当前值又被回填为候选项。
    pub(crate) fn remove_custom(&mut self, settings: &mut Settings, family: &str) -> bool {
        let before = settings.custom_font_families.len();
        settings
            .custom_font_families
            .retain(|kept| !kept.eq_ignore_ascii_case(family));
        let removed = Self::resolve(&self.installed, Some(family.to_string()));
        let mut changed = settings.custom_font_families.len() != before;
        for slot in [
            &mut settings.editor_font_family,
            &mut settings.tile_font_family,
        ] {
            let matched = slot.as_deref().is_some_and(|current| {
                current.eq_ignore_ascii_case(family)
                    || removed.as_deref().is_some_and(|removed| {
                        Self::resolve(&self.installed, Some(current.to_string()))
                            .as_deref()
                            .is_some_and(|resolved| resolved.eq_ignore_ascii_case(removed))
                    })
            });
            if matched {
                *slot = None;
                changed = true;
            }
        }
        if !changed {
            return false;
        }
        self.editor = Self::resolve(&self.installed, settings.editor_font_family.clone());
        self.tile = Self::resolve(&self.installed, settings.tile_font_family.clone());
        true
    }

    /// 写入排版位并同步缓存；返回是否有改动。
    fn write(&mut self, settings: &mut Settings, scope: FontScope, family: Option<String>) -> bool {
        let (slot, cached) = match scope {
            FontScope::Editor => (&mut settings.editor_font_family, &mut self.editor),
            FontScope::Tile => (&mut settings.tile_font_family, &mut self.tile),
        };
        if *slot == family {
            return false;
        }
        *slot = family.clone();
        *cached = family;
        true
    }

    /// 设置值解析为本机实际安装名；未安装或无值时存 `None`。
    fn resolve(installed: &[String], family: Option<String>) -> Option<String> {
        normalize_font_family(family)
            .and_then(|family| resolve_installed(installed, &family).map(str::to_string))
    }
}

/// 候选字体的显示名与系统字体族别名（按优先级）。
pub(crate) const READING_FONTS: &[(&str, &[&str])] = &[
    ("微软雅黑", &["Microsoft YaHei UI", "Microsoft YaHei"]),
    (
        "思源黑体",
        &[
            "Source Han Sans SC",
            "Source Han Sans CN",
            "Noto Sans SC",
            "Noto Sans CJK SC",
            "思源黑体",
        ],
    ),
    ("MiSans", &["MiSans"]),
    ("宋体", &["SimSun", "宋体"]),
    (
        "思源宋体",
        &[
            "Noto Serif SC",
            "Source Han Serif SC",
            "Source Han Serif CN",
            "思源宋体",
        ],
    ),
];

/// 候选项在本机实际安装的字体族名：显示名与别名按顺序取第一个可用。
/// 显示名也参与匹配，覆盖 DirectWrite 返回本地化族名的情况（如“微软雅黑”）。
pub(crate) fn installed_family<'a>(
    installed: &'a [String],
    label: &str,
    aliases: &[&str],
) -> Option<&'a str> {
    std::iter::once(label)
        .chain(aliases.iter().copied())
        .find_map(|name| installed_name(installed, name))
}

/// 已安装列表中的精确匹配（不区分 ASCII 大小写）。
fn installed_name<'a>(installed: &'a [String], family: &str) -> Option<&'a str> {
    installed
        .iter()
        .find(|name| name.eq_ignore_ascii_case(family))
        .map(String::as_str)
}

/// 把设置里保存的族名解析为本机实际安装名：
/// 先按原名精确匹配，再按候选别名匹配；未安装返回 `None`。
pub(crate) fn resolve_installed<'a>(installed: &'a [String], family: &str) -> Option<&'a str> {
    installed_name(installed, family).or_else(|| {
        READING_FONTS.iter().find_map(|(label, aliases)| {
            std::iter::once(*label)
                .chain(aliases.iter().copied())
                .any(|name| name.eq_ignore_ascii_case(family))
                .then(|| installed_family(installed, label, aliases))
                .flatten()
        })
    })
}

/// 内置候选中本机实际安装的字体族（用于系统字体选择窗标注"内置"）。
pub(crate) fn installed_builtins(installed: &[String]) -> Vec<String> {
    READING_FONTS
        .iter()
        .filter_map(|(label, aliases)| {
            installed_family(installed, label, aliases).map(str::to_string)
        })
        .collect()
}

/// 列表中是否已有该字体族（ASCII 大小写不敏感）。
pub(crate) fn contains_family(families: &[String], family: &str) -> bool {
    families
        .iter()
        .any(|name| name.eq_ignore_ascii_case(family))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    fn settings(editor: Option<&str>, tile: Option<&str>, custom: &[&str]) -> Settings {
        Settings {
            editor_font_family: editor.map(str::to_string),
            tile_font_family: tile.map(str::to_string),
            custom_font_families: custom.iter().map(|name| (*name).to_string()).collect(),
            ..Settings::default()
        }
    }

    #[test]
    fn installed_family_resolves_alias_order_and_localized_names() {
        let fonts = installed(&["Noto Sans SC", "Source Han Sans CN"]);
        assert_eq!(
            installed_family(&fonts, READING_FONTS[1].0, READING_FONTS[1].1),
            Some("Source Han Sans CN")
        );
        let localized = installed(&["微软雅黑"]);
        assert_eq!(
            installed_family(&localized, READING_FONTS[0].0, READING_FONTS[0].1),
            Some("微软雅黑")
        );
    }

    #[test]
    fn apply_adds_custom_font_and_sets_scope() {
        let mut settings = settings(None, None, &[]);
        let mut fonts = FontSettings::load(&settings, installed(&["JetBrains Mono"]));
        assert!(fonts.apply(
            &mut settings,
            FontScope::Editor,
            "jetbrains mono".to_string()
        ));
        assert_eq!(fonts.editor(), Some("JetBrains Mono"));
        assert_eq!(fonts.tile(), None);
        assert_eq!(
            settings.editor_font_family.as_deref(),
            Some("JetBrains Mono")
        );
        assert_eq!(settings.custom_font_families, vec!["JetBrains Mono"]);
    }

    #[test]
    fn apply_resolves_alias_to_installed_name() {
        let mut settings = settings(None, None, &[]);
        let mut fonts = FontSettings::load(&settings, installed(&["思源黑体"]));
        assert!(fonts.apply(
            &mut settings,
            FontScope::Tile,
            "Source Han Sans SC".to_string()
        ));
        assert_eq!(settings.tile_font_family.as_deref(), Some("思源黑体"));
        assert_eq!(settings.custom_font_families, vec!["思源黑体"]);
    }

    #[test]
    fn apply_uninstalled_font_leaves_settings_untouched() {
        let mut settings = settings(None, None, &[]);
        let mut fonts = FontSettings::load(&settings, installed(&["JetBrains Mono"]));
        let before = settings.clone();
        assert!(!fonts.apply(&mut settings, FontScope::Editor, "Missing Font".to_string()));
        assert_eq!(settings, before);
        assert_eq!(fonts.editor(), None);
    }

    #[test]
    fn apply_duplicate_font_does_not_append_again() {
        let mut settings = settings(Some("JetBrains Mono"), None, &["JetBrains Mono"]);
        let mut fonts = FontSettings::load(&settings, installed(&["JetBrains Mono"]));
        assert!(!fonts.apply(
            &mut settings,
            FontScope::Editor,
            "JetBrains Mono".to_string()
        ));
        assert_eq!(settings.custom_font_families, vec!["JetBrains Mono"]);
    }

    #[test]
    fn apply_updates_each_scope_independently() {
        let mut settings = settings(None, None, &[]);
        let mut fonts = FontSettings::load(&settings, installed(&["JetBrains Mono", "MiSans"]));
        assert!(fonts.apply(
            &mut settings,
            FontScope::Editor,
            "JetBrains Mono".to_string()
        ));
        assert!(fonts.apply(&mut settings, FontScope::Tile, "MiSans".to_string()));
        assert_eq!(fonts.editor(), Some("JetBrains Mono"));
        assert_eq!(fonts.tile(), Some("MiSans"));
        assert_eq!(
            settings.custom_font_families,
            vec!["JetBrains Mono", "MiSans"]
        );
    }

    #[test]
    fn remove_custom_resets_cached_slots() {
        let mut settings = settings(
            Some("JetBrains Mono"),
            Some("JetBrains Mono"),
            &["JetBrains Mono"],
        );
        let mut fonts = FontSettings::load(&settings, installed(&["JetBrains Mono"]));
        assert!(fonts.remove_custom(&mut settings, "JetBrains Mono"));
        assert_eq!(fonts.editor(), None);
        assert_eq!(fonts.tile(), None);
        assert_eq!(settings.editor_font_family, None);
        assert_eq!(settings.tile_font_family, None);
        assert!(settings.custom_font_families.is_empty());
    }

    #[test]
    fn remove_custom_clears_slot_saved_as_alias() {
        let mut settings = settings(Some("Source Han Sans SC"), None, &["思源黑体"]);
        let mut fonts = FontSettings::load(&settings, installed(&["思源黑体"]));
        assert!(fonts.remove_custom(&mut settings, "思源黑体"));
        assert_eq!(settings.editor_font_family, None);
        assert_eq!(fonts.editor(), None);
    }

    #[test]
    fn saved_family_resolves_through_aliases() {
        let fonts = installed(&["思源黑体"]);
        assert_eq!(
            resolve_installed(&fonts, "Source Han Sans SC"),
            Some("思源黑体")
        );
        assert_eq!(resolve_installed(&fonts, "思源黑体"), Some("思源黑体"));
        assert_eq!(resolve_installed(&fonts, "SimSun"), None);
    }

    #[test]
    fn exact_match_ignores_case() {
        let fonts = installed(&["Microsoft YaHei"]);
        assert_eq!(
            resolve_installed(&fonts, "microsoft yahei"),
            Some("Microsoft YaHei")
        );
    }
}
