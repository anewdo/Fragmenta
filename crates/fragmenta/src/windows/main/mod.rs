//! 主窗口 shell

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::animation::{EffectTransition, ease_in_out_cubic};
use gpui_kit::component::{ActiveTheme as _, Icon, TitleBar, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::time::Duration;

use fragmenta_core::settings::FontScale;

use crate::notes::NotesPage;
use crate::settings::SettingsPage;
use crate::todos::TodosPage;
use crate::ui::layout::LayoutClass;
use crate::ui::tokens::{self, TextRole, TextRoleExt as _};
use crate::widgets::toast::{ToastState, ToastStore};
use crate::widgets::window_title_bar;

/// 侧栏展开 / 收起宽度动画时长
const SIDEBAR_TRANSITION_DURATION: Duration = Duration::from_millis(200);

/// 主窗口单例登记：窗口 handle 存于 App 全局，重复 `open` 激活既有窗口（D7）。
struct MainWindow {
    handle: AnyWindowHandle,
    _closed_sub: Subscription,
}

impl Global for MainWindow {}

/// 页面路由；各页 Entity 常驻于 shell，切换保留页面状态（架构 §5.8）。
/// 当前页由侧栏菜单高亮指明，无需文字大标题。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Notes,
    Todos,
    Settings,
}

/// 侧栏拖宽的拖拽值：仅作 `on_drag` 类型标记，不带数据。
struct SidebarResizeDrag;

/// 拖宽发起时的幽灵视图：gpui 拖拽协议需要一个 `Render` 载体，渲染为空，
/// 拖动全程由 `on_drag_move` 承载。
struct SidebarResizeGhost;

impl Render for SidebarResizeGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// 打开主窗口；已开则激活既有窗口（单例幂等，D7）。
pub fn open(cx: &mut App) {
    open_handle(cx);
}

/// 打开或激活主窗口，并返回可承载退出提示的窗口。
pub(crate) fn open_handle(cx: &mut App) -> Option<AnyWindowHandle> {
    if let Some(handle) = cx.try_global::<MainWindow>().map(|reg| reg.handle) {
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

    let bounds = Bounds::centered(
        None,
        size(tokens::widths::MAIN_WINDOW, tokens::heights::MAIN_WINDOW),
        cx,
    );
    let mut options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(
            tokens::widths::MAIN_WINDOW_MIN,
            tokens::heights::MAIN_WINDOW_MIN,
        )),
        ..TitleBar::window_options()
    };
    // TitleBar 自绘不展示标题文字；任务栏 / Alt-Tab 仍显示应用名
    if let Some(titlebar) = options.titlebar.as_mut() {
        titlebar.title = Some("Fragmenta".into());
    }
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| MainShell::new(window, cx))
    }) {
        Ok((handle, _)) => {
            let closed_sub = cx.on_window_closed(move |cx, closed_id| {
                let is_current = cx
                    .try_global::<MainWindow>()
                    .is_some_and(|main| main.handle.window_id() == closed_id);
                if is_current && closed_id == handle.window_id() {
                    cx.remove_global::<MainWindow>();
                    let settings = cx.global::<crate::settings::SettingsStore>().0.clone();
                    settings.update(cx, |state, _| state.shrink_database_memory());
                    cx.defer(|_| {
                        crate::win32::trim_working_set();
                    });
                }
            });
            cx.set_global(MainWindow {
                handle,
                _closed_sub: closed_sub,
            });
            Some(handle)
        }
        Err(err) => {
            eprintln!("[fragmenta] 打开主窗口失败: {err:?}");
            None
        }
    }
}

/// 主窗口 shell：自绘标题栏 + 侧栏导航（可拖宽 / 可收起，带动画）+
/// 三页常驻 Entity 路由。
struct MainShell {
    page: Page,
    sidebar_collapsed: bool,
    /// 侧栏拖宽进行中：提示线转深蓝；随拖拽结束（on_drop）复位。
    sidebar_resizing: bool,
    /// 侧栏展开宽度（180–420px 钳制）：自持于 shell，收起 / 展开不丢失。
    sidebar_width: Pixels,
    /// 宽度动画起点：仅在收起状态切换时记录当前可见宽度。
    sidebar_anim_from: Pixels,
    /// 宽度动画标识：切换收起状态时递增，拖宽时不重启动画。
    sidebar_anim_seq: u64,
    notes: Entity<NotesPage>,
    todos: Entity<TodosPage>,
    settings: Entity<SettingsPage>,
    /// toast 浮层（如导出结果报告）：toast 状态变化时重绘 shell。
    toast: Entity<ToastState>,
    _appearance_sub: Subscription,
    _toast_sub: Subscription,
}

impl MainShell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = cx.global::<crate::settings::SettingsStore>().0.clone();
        let appearance_sub =
            settings.update(cx, |state, cx| state.attach_appearance_bridge(window, cx));
        // 主窗口关闭语义（架构 §5.8）：close_to_quit 开 → 统一退出路径
        // （保存失败时保留窗口）；关（默认）→ 窗口关闭但进程托盘驻留
        // （QuitMode::Explicit），热键照常呼出磁贴，托盘可再开主窗口。
        // TitleBar 关闭按钮在 Windows 经原生关闭路径，同样走到这里。
        window.on_window_should_close(cx, |_, cx| {
            let settings = cx.global::<crate::settings::SettingsStore>().0.clone();
            if settings.read(cx).close_to_quit() || crate::app::quit::is_pending(cx) {
                cx.defer(crate::app::quit::request_quit);
                return false;
            }
            true
        });
        let toast = cx.global::<ToastStore>().0.clone();
        let toast_sub = cx.observe(&toast, |_, _, cx| cx.notify());
        let sidebar_width = settings
            .read(cx)
            .sidebar_width()
            .clamp(tokens::widths::SIDEBAR_MIN, tokens::widths::SIDEBAR_MAX);
        // 紧凑档首次进入自动收起一次；此后窗口缩放不再改写用户状态。
        let compact = LayoutClass::from_width(window.viewport_size().width) == LayoutClass::Compact;
        Self {
            page: Page::Notes,
            sidebar_collapsed: compact,
            sidebar_resizing: false,
            sidebar_width,
            // 首次渲染不走动画：起点即当前可见宽度
            sidebar_anim_from: if compact {
                tokens::widths::SIDEBAR_COLLAPSED
            } else {
                sidebar_width
            },
            sidebar_anim_seq: 0,
            notes: cx.new(|cx| NotesPage::new(window, cx)),
            todos: cx.new(|cx| TodosPage::new(window, cx)),
            settings: cx.new(|cx| SettingsPage::new(window, cx)),
            toast,
            _appearance_sub: appearance_sub,
            _toast_sub: toast_sub,
        }
    }

    /// 侧栏导航项：图标 + 标题（控件标签角色）+ 激活态，点击切换路由。
    /// 收起时隐藏标题，图标在栏内居中。
    fn nav_item(
        &self,
        page: Page,
        label: &'static str,
        icon: IconName,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.page == page;
        let accent = cx.theme().sidebar_accent;
        let accent_fg = cx.theme().sidebar_accent_foreground;
        h_flex()
            .id(("sidebar-nav", page as usize))
            .w_full()
            .h(tokens::heights::NAV_ITEM)
            .px(tokens::spacing::MD)
            .gap_x(tokens::spacing::SM)
            .rounded(tokens::radius::SM)
            .items_center()
            .cursor_pointer()
            .when(collapsed, |this| this.justify_center())
            .when(active, |this| {
                this.font_medium().bg(accent).text_color(accent_fg)
            })
            .when(!active, |this| {
                this.hover(move |style| style.bg(accent.opacity(0.8)).text_color(accent_fg))
            })
            .child(Icon::new(icon).size_5())
            .when(!collapsed, |this| {
                this.child(
                    div()
                        .text_role(TextRole::ControlLabel, FontScale::Medium)
                        .child(label),
                )
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.page = page;
                cx.notify();
            }))
    }
}

impl Render for MainShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = self.sidebar_collapsed;
        // 收起开关驻留侧栏头部；展开时贴右，收起时居中
        let toggle_icon = if collapsed {
            IconName::PanelLeftOpen
        } else {
            IconName::PanelLeftClose
        };
        let toggle = div()
            .id("sidebar-toggle")
            .flex()
            .items_center()
            .justify_center()
            .size(tokens::heights::CONTROL)
            .rounded(tokens::radius::SM)
            .cursor_pointer()
            .hover(|this| this.bg(cx.theme().sidebar_accent.opacity(0.8)))
            .child(Icon::new(toggle_icon).size_5())
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.sidebar_anim_from = if this.sidebar_collapsed {
                    tokens::widths::SIDEBAR_COLLAPSED
                } else {
                    this.sidebar_width
                };
                this.sidebar_collapsed = !this.sidebar_collapsed;
                this.sidebar_anim_seq = this.sidebar_anim_seq.wrapping_add(1);
                cx.notify();
            }));
        // 自绘侧栏骨架：kit 侧栏的收起宽度写死 48px，这里由
        // tokens::widths::SIDEBAR_COLLAPSED 决定。锚定左缘，宽度动画在
        // 展开宽 ↔ 收起宽之间过渡；展开期间宽度即拖宽结果。
        let sidebar = v_flex()
            .id("main-sidebar")
            .flex_shrink_0()
            .h_full()
            .overflow_hidden()
            .bg(cx.theme().tokens.sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(
                h_flex()
                    .w_full()
                    .pt(tokens::spacing::MD)
                    .px(tokens::spacing::MD)
                    .when(!collapsed, |this| this.justify_end())
                    .when(collapsed, |this| this.justify_center())
                    .child(toggle),
            )
            .child(
                v_flex()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .pt(tokens::spacing::MD)
                    .px(tokens::spacing::MD)
                    .gap_y(tokens::spacing::SM)
                    .children([
                        self.nav_item(Page::Notes, "笔记", IconName::NotebookPen, collapsed, cx),
                        self.nav_item(Page::Todos, "待办", IconName::SquareCheckBig, collapsed, cx),
                        self.nav_item(Page::Settings, "设置", IconName::Settings, collapsed, cx),
                    ]),
            );
        let target_width = if collapsed {
            tokens::widths::SIDEBAR_COLLAPSED
        } else {
            self.sidebar_width
        };
        let sidebar = EffectTransition::new(SIDEBAR_TRANSITION_DURATION)
            .ease(ease_in_out_cubic)
            .width(self.sidebar_anim_from, target_width)
            .apply(sidebar, ("sidebar-width", self.sidebar_anim_seq))
            .into_any_element();

        // 拖宽手柄：骑缝式命中带——absolute 横跨接缝两侧（各 4px），不占
        // 布局空间，菜单栏与内容区零间隙衔接；拖动期间宽度跟随指针 x
        // （侧栏锚定窗口左缘），180–420px 钳制。
        // 提示线为命中带的兄弟元素（content 相对坐标），2px 骑缝（盖住
        // 侧栏边框像素 + 1px），与视觉边缘严丝合缝：平时隐形，悬停浅蓝，
        // 拖拽中深蓝
        let handle = div()
            .id("sidebar-resize-events")
            .absolute()
            .top_0()
            .left_0()
            .w_0()
            .h_full()
            .child(
                gpui_kit::base::resize_handle("sidebar-resize", Axis::Horizontal)
                    .with_appearance(gpui_kit::component::resize_handle_appearance())
                    .on_drag(SidebarResizeDrag, |_, _, _, cx| {
                        cx.new(|_| SidebarResizeGhost)
                    }),
            )
            .on_drag_move::<SidebarResizeDrag>(cx.listener(
                |this, ev: &DragMoveEvent<SidebarResizeDrag>, _, cx| {
                    this.sidebar_resizing = true;
                    let x = ev.event.position.x;
                    let width = if x < tokens::widths::SIDEBAR_MIN {
                        tokens::widths::SIDEBAR_MIN
                    } else if x > tokens::widths::SIDEBAR_MAX {
                        tokens::widths::SIDEBAR_MAX
                    } else {
                        x
                    };
                    this.sidebar_width = width;
                    cx.notify();
                },
            ))
            // 拖拽结束检测：带内 `on_mouse_up` 与带外 `on_mouse_up_out`
            // 条件互补，任意左键释放必触其一（on_drop 需要下层 hitbox
            // 未被本带 occlude 遮蔽，不可靠，故弃用）
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if this.sidebar_resizing {
                        this.sidebar_resizing = false;
                        let width = this.sidebar_width;
                        let settings = cx.global::<crate::settings::SettingsStore>().0.clone();
                        settings.update(cx, |state, _| state.set_sidebar_width(width));
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if this.sidebar_resizing {
                        this.sidebar_resizing = false;
                        let width = this.sidebar_width;
                        let settings = cx.global::<crate::settings::SettingsStore>().0.clone();
                        settings.update(cx, |state, _| state.set_sidebar_width(width));
                        cx.notify();
                    }
                }),
            );

        let content = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .child(
                div().flex_1().min_h_0().child(match self.page {
                    Page::Notes => self.notes.clone().into_any_element(),
                    Page::Todos => self.todos.clone().into_any_element(),
                    Page::Settings => self.settings.clone().into_any_element(),
                }),
            )
            // 骑缝手柄与提示线挂在内容列左缘（absolute，不占布局空间），
            // 仅展开态
            .when(!collapsed, |this| this.child(handle));

        v_flex()
            .relative()
            .size_full()
            .bg(cx.theme().background)
            // 顶部通栏 client-side 标题栏：应用图标 + 应用名；最右 = 窗口控制
            .child(window_title_bar::window_title_bar(
                h_flex()
                    .items_center()
                    .gap(tokens::spacing::SM)
                    .child(Icon::new(IconName::Layers))
                    .child(
                        div()
                            .text_role(TextRole::ControlLabel, FontScale::Medium)
                            .child("Fragmenta"),
                    ),
                window,
                cx,
            ))
            // 中段：侧栏（收起 / 展开带 kit 宽度动画）与内容区紧密衔接，
            // 分隔 / 拖宽由骑缝手柄承担
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar)
                    .child(content),
            )
            // toast 浮层最后挂载：后绘制的 absolute 浮层盖在页面内容之上
            .child(self.toast.clone())
    }
}
