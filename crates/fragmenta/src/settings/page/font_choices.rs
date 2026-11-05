//! 字体下拉候选：内置常用阅读字体 + 用户从系统字体中添加的字体，
//! 未安装项禁用并在右侧标注。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::searchable_list::{
    SearchableListDelegate, SearchableListItem, SearchableVec,
};
use gpui_kit::component::{ActiveTheme as _, IndexPath, Sizable as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, SharedString, Task, Window, div};

use crate::settings::SettingsStore;
use crate::settings::fonts;
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};

/// "跟随系统"哨兵项显示名；选中该项时设置存 `None`。
pub(super) const DEFAULT_LABEL: &str = "系统默认";

/// 未安装标记：仅作提示，选项本身禁用。
const MISSING_LABEL: &str = "未安装";

/// 下拉选项：显示名、保存到设置文件的字体族（`None` 为跟随系统）、安装
/// 状态与是否可删除（仅用户从系统字体中添加的项）。
#[derive(Clone)]
pub(super) struct FontChoice {
    pub label: SharedString,
    pub family: Option<String>,
    pub installed: bool,
    pub removable: bool,
}

impl SearchableListItem for FontChoice {
    type Value = Option<String>;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.family
    }

    fn disabled(&self) -> bool {
        !self.installed
    }
}

/// 下拉数据源：默认的行渲染会把内容包在内容宽度的盒子里，删除按钮无法
/// 贴住行尾；这里接管行渲染，让行内容铺满可用宽度。
pub(super) struct FontChoices(SearchableVec<FontChoice>);

impl FontChoices {
    pub(super) fn new(choices: Vec<FontChoice>) -> Self {
        Self(SearchableVec::new(choices))
    }
}

impl SearchableListDelegate for FontChoices {
    type Item = FontChoice;

    fn items_count(&self, section: usize) -> usize {
        self.0.items_count(section)
    }

    fn item(&self, ix: IndexPath) -> Option<&Self::Item> {
        self.0.item(ix)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        Self::Item: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.0.position(value)
    }

    fn perform_search(&mut self, query: &str, window: &mut Window, cx: &mut App) -> Task<()> {
        self.0.perform_search(query, window, cx)
    }

    fn render_item(
        &self,
        ix: IndexPath,
        item: &Self::Item,
        checked: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        Some(font_choice_row(ix.row, item, checked, cx))
    }
}

/// 单行内容：名称、未安装标记与删除按钮依次靠右排列；选中状态由行底色
/// 表达，不再额外画对勾。
fn font_choice_row(row: usize, choice: &FontChoice, _checked: bool, cx: &mut App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .w_full()
        .items_center()
        .gap(tokens::spacing::SM)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(choice.label.clone()),
        )
        .when(!choice.installed, |el| {
            el.child(
                div()
                    .flex_shrink_0()
                    .text_role(TextRole::Meta)
                    .text_color(muted)
                    .child(MISSING_LABEL),
            )
        })
        .when(choice.removable, |el| {
            el.child(remove_button(
                row,
                choice.family.clone().unwrap_or_default(),
            ))
        })
        .into_any_element()
}

/// 自定义项的删除按钮：ghost 图标，无悬停底色；点击只移除候选，事件不再
/// 冒泡到行上触发选中 / 收起下拉。
fn remove_button(row: usize, family: String) -> impl IntoElement {
    Button::new(("font-choice-remove", row))
        .ghost()
        .xsmall()
        .icon(IconName::X)
        .tooltip("删除")
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let store = cx.global::<SettingsStore>().0.clone();
            store.update(cx, |state, cx| state.remove_custom_font(&family, cx));
        })
}

/// 组装下拉选项：首项跟随系统，其后列出全部内置阅读字体，再追加用户
/// 从系统字体中添加的字体；本机未安装的项保持可见但禁用。已保存但不在
/// 候选内的可用字体追加一项，保证往返显示不丢失。
pub(super) fn choices(
    installed: &[String],
    custom: &[String],
    current: Option<&str>,
) -> Vec<FontChoice> {
    let mut choices = vec![FontChoice {
        label: SharedString::from(DEFAULT_LABEL),
        family: None,
        installed: true,
        removable: false,
    }];
    let mut seen: Vec<String> = Vec::new();
    for (label, aliases) in fonts::READING_FONTS {
        let family = fonts::installed_family(installed, label, aliases).map(str::to_string);
        if family
            .as_deref()
            .is_some_and(|family| fonts::contains_family(&seen, family))
        {
            continue;
        }
        if let Some(family) = &family {
            seen.push(family.clone());
        }
        choices.push(FontChoice {
            label: SharedString::from(*label),
            installed: family.is_some(),
            family,
            removable: false,
        });
    }
    for family in custom {
        let family = fonts::resolve_installed(installed, family)
            .map(str::to_string)
            .unwrap_or_else(|| family.clone());
        if fonts::contains_family(&seen, &family) {
            continue;
        }
        seen.push(family.clone());
        choices.push(FontChoice {
            label: SharedString::from(family.clone()),
            installed: fonts::contains_family(installed, &family),
            family: Some(family),
            removable: true,
        });
    }
    if let Some(current) = current
        && !choices
            .iter()
            .any(|choice| choice.family.as_deref() == Some(current))
    {
        choices.push(FontChoice {
            label: SharedString::from(current),
            family: Some(current.to_string()),
            installed: true,
            removable: false,
        });
    }
    choices
}

/// 当前字体在选项中的位置；未命中时落在首项（跟随系统）。
pub(super) fn label_index(choices: &[FontChoice], family: Option<&str>) -> usize {
    choices
        .iter()
        .position(|choice| choice.family.as_deref() == family)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn custom_choices_appear_only_after_adding_and_are_removable() {
        // 未加入自定义列表的系统字体不会自动出现在下拉中
        let installed = names(&["Microsoft YaHei", "JetBrains Mono"]);
        let empty = choices(&installed, &[], None);
        assert!(!empty.iter().any(|c| c.label.as_ref() == "JetBrains Mono"));

        let custom = names(&["JetBrains Mono"]);
        let candidates = choices(&installed, &custom, None);
        let removable: Vec<&str> = candidates
            .iter()
            .filter(|choice| choice.removable)
            .map(|choice| choice.label.as_ref())
            .collect();
        assert_eq!(removable, ["JetBrains Mono"]);
    }

    #[test]
    fn custom_fonts_append_after_builtins() {
        let installed = names(&["Microsoft YaHei", "JetBrains Mono", "霞鹜文楷"]);
        let custom = names(&["JetBrains Mono", "霞鹜文楷"]);
        let candidates = choices(&installed, &custom, None);
        let labels: Vec<&str> = candidates.iter().map(|c| c.label.as_ref()).collect();
        assert_eq!(labels[0], DEFAULT_LABEL);
        // 自定义项按添加顺序排在全部内置候选之后
        assert_eq!(
            &labels[labels.len() - custom.len()..],
            &["JetBrains Mono", "霞鹜文楷"][..]
        );
    }

    #[test]
    fn duplicate_custom_family_is_skipped() {
        // 自定义列表与内置解析结果重复（大小写不敏感）时不重复出现，
        // 也不会留下可删除的重复行
        let installed = names(&["Microsoft YaHei"]);
        let custom = names(&["microsoft yahei", "Microsoft YaHei"]);
        let candidates = choices(&installed, &custom, None);
        let duplicated: Vec<&FontChoice> = candidates
            .iter()
            .filter(|c| c.family.as_deref() == Some("Microsoft YaHei"))
            .collect();
        assert_eq!(duplicated.len(), 1);
        assert!(!duplicated[0].removable);
    }

    #[test]
    fn missing_custom_font_stays_visible_but_disabled() {
        let installed = names(&["Arial"]);
        let custom = names(&["Gone Font"]);
        let choices = choices(&installed, &custom, None);
        let gone = choices
            .iter()
            .find(|c| c.label.as_ref() == "Gone Font")
            .expect("自定义字体保留在候选中");
        assert!(!gone.installed);
    }

    #[test]
    fn current_family_outside_choices_is_appended() {
        let installed = names(&["Arial"]);
        // 精确匹配未在候选内：追加保存值，保证下拉能显示当前字体
        let choices = choices(&installed, &[], Some("Arial"));
        assert_eq!(
            choices
                .iter()
                .filter(|c| c.family.as_deref() == Some("Arial"))
                .count(),
            1
        );
        assert_eq!(
            choices.last().map(|c| c.family.as_deref()),
            Some(Some("Arial"))
        );
    }
}
