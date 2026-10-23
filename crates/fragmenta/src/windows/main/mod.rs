//! 主窗口 shell（架构 §5.8 / D7）：单例幂等 + `Page` 路由 + 四页常驻。
//!
//! 经 kit 门面 `gpui_kit::open_window` 开窗（自动包 `base::Root`，获得
//! overlay hosting）；业务页面由笔记、待办、设置功能模块提供。

mod home;

use gpui_kit::assets::IconName;
use gpui_kit::component::sidebar::{Sidebar, SidebarMenu, SidebarMenuItem, SidebarToggleButton};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::notes::NotesPage;
use crate::settings::SettingsPage;
use crate::todos::TodosPage;
use home::HomePage;

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
    open_handle(cx);
}

/// 打开或激活主窗口，并返回可承载退出提示的窗口。
pub(crate) fn open_handle(cx: &mut App) -> Option<AnyWindowHandle> {
    if let Some(handle) = cx.try_global::<MainWindow>().map(|reg| reg.0) {
        // 窗口仍存活则激活；已关闭时登记表过期，走重开路径
        if cx.windows().contains(&handle)
            && let Some(typed) = handle.downcast::<gpui_kit::base::Root>()
            && typed
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return Some(handle);
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
        Ok((handle, _)) => {
            cx.set_global(MainWindow(handle));
            Some(handle)
        }
        Err(err) => {
            eprintln!("[fragmenta] 打开主窗口失败: {err:?}");
            None
        }
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
    _appearance_sub: Subscription,
    _notes_sub: Subscription,
}

impl MainShell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = cx.global::<crate::settings::SettingsStore>().0.clone();
        let appearance_sub =
            settings.update(cx, |state, cx| state.attach_appearance_bridge(window, cx));
        // 主窗口关闭语义（架构 §5.8）：close_to_quit 开 → 统一退出路径
        // （保存失败时保留窗口）；关（默认）→ 窗口关闭但进程托盘驻留
        // （QuitMode::Explicit），热键照常呼出磁贴，托盘可再开主窗口。
        window.on_window_should_close(cx, |_, cx| {
            let settings = cx.global::<crate::settings::SettingsStore>().0.clone();
            if settings.read(cx).close_to_quit() || crate::app::quit::is_pending(cx) {
                cx.defer(crate::app::quit::request_quit);
                return false;
            }
            true
        });
        let notes = cx.new(|cx| NotesPage::new(window, cx));
        let notes_sub = cx.observe(&notes, |_, _, cx| cx.notify());
        Self {
            page: Page::Home,
            sidebar_collapsed: false,
            home: cx.new(|_| HomePage),
            notes,
            todos: cx.new(|cx| TodosPage::new(window, cx)),
            settings: cx.new(|cx| SettingsPage::new(window, cx)),
            _appearance_sub: appearance_sub,
            _notes_sub: notes_sub,
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
        // 笔记页标题栏右侧槽位（模式切换组）先于元素链构建，避免借用交叉
        let notes_header_slot = (self.page == Page::Notes)
            .then(|| self.notes.update(cx, |page, cx| page.header_actions(cx)));
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
                            // 标题栏行：以 title_bar token 着色（色系主题下为深调 chrome）
                            .bg(cx.theme().title_bar)
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(SidebarToggleButton::new().collapsed(collapsed).on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.sidebar_collapsed = !this.sidebar_collapsed;
                                    cx.notify();
                                }),
                            ))
                            .child(self.page.title())
                            .when_some(notes_header_slot, |el, slot| {
                                // 与页面标题同容器，靠右对齐（px_3 提供与边界的间距）
                                el.child(div().flex_1().min_w(px(24.))).child(slot)
                            }),
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
