//! 排版字体候选：只列文档阅读常用字体，按本机安装情况过滤。

use gpui_kit::{App, SharedString};

/// "跟随系统"哨兵项显示名；选中该项时设置存 `None`。
pub(super) const DEFAULT_LABEL: &str = "系统默认";

/// 阅读与编辑器字体候选：显示名 + 系统字体族别名（取本机已安装的第一个别名）。
const READING_FONTS: &[(&str, &[&str])] = &[
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
    ("等线", &["DengXian", "等线"]),
    (
        "更纱黑体",
        &["Sarasa Gothic SC", "Sarasa Fixed SC", "更纱黑体"],
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
    ("仿宋", &["FangSong", "仿宋"]),
    ("楷体", &["KaiTi", "楷体"]),
    ("霞鹜文楷", &["LXGW WenKai", "LXGW WenKai GB"]),
];

/// 下拉选项：显示名与保存到设置文件的字体族（`None` 为跟随系统）。
#[derive(Clone)]
pub(super) struct FontChoice {
    pub label: SharedString,
    pub family: Option<String>,
}

/// 组装下拉选项：首项跟随系统，其后仅保留本机已安装的阅读字体；
/// 已保存但不在候选内的字体追加一项，保证往返显示不丢失。
pub(super) fn choices(cx: &App, current: Option<&str>) -> Vec<FontChoice> {
    let installed: Vec<String> = cx.text_system().all_font_names();
    let mut choices = vec![FontChoice {
        label: SharedString::from(DEFAULT_LABEL),
        family: None,
    }];
    for (label, aliases) in READING_FONTS {
        let family = aliases.iter().find_map(|alias| {
            let alias = alias.to_lowercase();
            installed
                .iter()
                .find(|name| name.to_lowercase() == alias)
                .cloned()
        });
        if let Some(family) = family {
            choices.push(FontChoice {
                label: SharedString::from(*label),
                family: Some(family),
            });
        }
    }
    if let Some(current) = current {
        if !choices
            .iter()
            .any(|choice| choice.family.as_deref() == Some(current))
        {
            choices.push(FontChoice {
                label: SharedString::from(current),
                family: Some(current.to_string()),
            });
        }
    }
    choices
}

/// 下拉展示项。
pub(super) fn labels(choices: &[FontChoice]) -> Vec<SharedString> {
    choices.iter().map(|choice| choice.label.clone()).collect()
}

/// 当前字体在选项中的位置；未命中时落在首项（跟随系统）。
pub(super) fn label_index(choices: &[FontChoice], family: Option<&str>) -> usize {
    choices
        .iter()
        .position(|choice| choice.family.as_deref() == family)
        .unwrap_or(0)
}

/// 选中项显示名 → 保存的字体族。
pub(super) fn family_of(choices: &[FontChoice], label: &str) -> Option<Option<String>> {
    choices
        .iter()
        .find(|choice| choice.label.as_ref() == label)
        .map(|choice| choice.family.clone())
}
