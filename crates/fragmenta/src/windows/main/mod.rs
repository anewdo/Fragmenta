//! 主窗口 shell（架构 §5.8 / D7）：单例幂等 + `Page` 路由 + 四页常驻。
//!
//! 经 kit 门面 `gpui_kit::open_window` 开窗（自动包 `base::Root`，获得
//! overlay hosting）；四页为 Phase 7 交付的空白占位页，实装见各页模块。

mod home;
mod notes;
mod settings;
mod todos;

use gpui_kit::assets::IconName;
use gpui_kit::component::sidebar::{Sidebar, SidebarMenu, SidebarMenuItem, SidebarToggleButton};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use home::HomePage;
use notes::NotesPage;
use settings::SettingsPage;
use todos::TodosPage;

/// 主窗口单例登记：窗口 handle 存于 App 全局，重复 `open` 激活既有窗口（D7）。
struct MainWindow(AnyWindowHandle);

impl Global for MainWindow {}

/// 页面路由；各页 Entity 常驻于 shell，切换保留页面状态（架构 §5.8）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Notes,
    Todos,
    Settings,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::Home => "首页",
            Page::Notes => "笔记",
            Page::Todos => "待办",
            Page::Settings => "设置",
        }
    }
}

/// 打开主窗口；已开则激活既有窗口（单例幂等，D7）。
pub fn open(cx: &mut App) {
    if let Some(handle) = cx.try_global::<MainWindow>().map(|reg| reg.0) {
        // 窗口仍存活则激活；已关闭时登记表过期，走重开路径
        if cx.windows().contains(&handle)
            && let Some(typed) = handle.downcast::<gpui_kit::base::Root>()
            && typed
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
    }

    let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Fragmenta".into()),
            ..Default::default()
        }),
        window_min_size: Some(size(px(760.), px(520.))),
        ..Default::default()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| MainShell::new(window, cx))
    }) {
        Ok((handle, _)) => cx.set_global(MainWindow(handle)),
        Err(err) => eprintln!("[fragmenta] 打开主窗口失败: {err:?}"),
    }
}

/// 主窗口 shell：kit `Sidebar` 导航 + 四页常驻 Entity 路由。
struct MainShell {
    page: Page,
    sidebar_collapsed: bool,
    home: Entity<HomePage>,
    notes: Entity<NotesPage>,
    todos: Entity<TodosPage>,
    settings: Entity<SettingsPage>,
}

impl MainShell {
    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            page: Page::Home,
            sidebar_collapsed: false,
            home: cx.new(|_| HomePage),
            notes: cx.new(|_| NotesPage),
            todos: cx.new(|_| TodosPage),
            settings: cx.new(|_| SettingsPage),
        }
    }

    /// 侧栏导航项：图标 + 标题 + 激活态，点击切换路由。
    fn nav_item(
        &self,
        page: Page,
        label: &'static str,
        icon: IconName,
        cx: &mut Context<Self>,
    ) -> SidebarMenuItem {
        SidebarMenuItem::new(label)
            .icon(Icon::new(icon))
            .active(self.page == page)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.page = page;
                cx.notify();
            }))
    }
}

impl Render for MainShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = self.sidebar_collapsed;
        div()
            .flex()
            .size_full()
            .bg(cx.theme().background)
            .child(
                Sidebar::new("main-sidebar")
                    .collapsible(true)
                    .collapsed(collapsed)
                    .header(
                        h_flex()
                            .gap_2()
                            .child(Icon::new(IconName::Layers))
                            .when(!collapsed, |el| el.child("Fragmenta")),
                    )
                    .child(SidebarMenu::new().children([
                        self.nav_item(Page::Home, "首页", IconName::House, cx),
                        self.nav_item(Page::Notes, "笔记", IconName::NotebookPen, cx),
                        self.nav_item(Page::Todos, "待办", IconName::SquareCheckBig, cx),
                        self.nav_item(Page::Settings, "设置", IconName::Settings, cx),
                    ])),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .h_10()
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(SidebarToggleButton::new().collapsed(collapsed).on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.sidebar_collapsed = !this.sidebar_collapsed;
                                    cx.notify();
                                }),
                            ))
                            .child(self.page.title()),
                    )
                    .child(div().flex_1().min_h_0().child(match self.page {
                        Page::Home => self.home.clone().into_any_element(),
                        Page::Notes => self.notes.clone().into_any_element(),
                        Page::Todos => self.todos.clone().into_any_element(),
                        Page::Settings => self.settings.clone().into_any_element(),
                    })),
            )
    }
}
