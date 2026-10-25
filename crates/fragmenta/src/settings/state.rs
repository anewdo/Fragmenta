use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fragmenta_core::db::Db;
use fragmenta_core::settings::{FontScale, Settings, ThemeMode};
use global_hotkey::hotkey::HotKey;
use gpui_kit::component::{ActiveTheme as _, ThemeMode as KitThemeMode};
use gpui_kit::{
    Context, Entity, EventEmitter, Global, Pixels, Subscription, Task, Window, WindowAppearance, px,
};

use crate::autostart::AutostartError;
use crate::editor::ImageStore;
use crate::hotkey::{HotkeyController, HotkeyError};
use crate::notes::{NotesStore, SaveStatus};
use crate::settings::migrate::{self, DataDirError};
use crate::ui::theme;

/// App 级设置 store：运行期设置修改的唯一入口（D8 副作用集中）。
pub(crate) struct SettingsStore(pub(crate) Entity<SettingsState>);

impl Global for SettingsStore {}

/// 设置变更广播：订阅方（设置页等）据此刷新展示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEvent {
    Changed,
}

impl EventEmitter<SettingsEvent> for SettingsState {}

/// 设置状态：持有当前 `Settings`、settings.json 路径与共享数据库连接。
pub struct SettingsState {
    settings: Settings,
    path: PathBuf,
    /// 共享数据库门面（与 notes / todos store 同一 `Arc<Db>`），
    /// 供 `set_data_dir` 迁移编排在线备份与连接热替换。
    db: Arc<Db>,
    /// 磁贴尺寸防抖写盘任务；`None` 即无待写内容。
    tile_persist: Option<Task<()>>,
    /// 应用退出兜底：退出前落盘未到期的磁贴尺寸。
    _quit_sub: Subscription,
}

/// 磁贴尺寸写盘防抖 原生 resize 拖拽期间 WM_SIZE
/// 连续触发 bounds 观察，落盘按此窗口合并为拖拽结束后的最后一次。
const TILE_SIZE_DEBOUNCE: Duration = Duration::from_millis(400);

impl SettingsState {
    /// 构造即按当前设置应用主题模式（启动期主题应用唯一入口）。
    pub fn new(settings: Settings, path: PathBuf, db: Arc<Db>, cx: &mut Context<Self>) -> Self {
        theme::init(cx);
        let quit_sub = cx.on_app_quit(|state, _| {
            state.flush_tile_persist();
            async {}
        });
        let mut state = Self {
            settings,
            path,
            db,
            tile_persist: None,
            _quit_sub: quit_sub,
        };
        state.apply_theme_mode(cx);
        state
    }

    /// 当前数据目录。
    pub fn data_dir(&self) -> &Path {
        &self.settings.data_dir
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

    /// 开机自启动开关当前值；默认 false。
    pub fn autostart(&self) -> bool {
        self.settings.autostart
    }

    pub fn sidebar_width(&self) -> Pixels {
        px(self.settings.layout.sidebar_width)
    }

    pub fn notes_list_width(&self) -> Pixels {
        px(self.settings.layout.notes_list_width)
    }

    pub fn editor_source_width(&self) -> Pixels {
        px(self.settings.layout.editor_source_width)
    }

    pub fn set_sidebar_width(&mut self, width: Pixels) {
        self.settings.layout.sidebar_width = width.as_f32();
        self.persist();
    }

    pub fn set_notes_list_width(&mut self, width: Pixels) {
        self.settings.layout.notes_list_width = width.as_f32();
        self.persist();
    }

    pub fn set_editor_source_width(&mut self, width: Pixels) {
        self.settings.layout.editor_source_width = width.as_f32();
        self.persist();
    }

    /// 磁贴窗口尺寸（逻辑像素）：呼出新磁贴的打开尺寸。
    pub fn tile_size(&self) -> (Pixels, Pixels) {
        (
            px(self.settings.layout.tile_width),
            px(self.settings.layout.tile_height),
        )
    }

    /// 记录用户调整后的磁贴尺寸（磁贴视图 bounds 观察回调提交），
    /// 防抖写盘。窗口拖动（仅位移）与跨显示器 scale 变化不改变逻辑
    /// 尺寸，同值提交直接跳过。
    pub fn set_tile_size(&mut self, width: Pixels, height: Pixels, cx: &mut Context<Self>) {
        let (w, h) = (width.as_f32(), height.as_f32());
        if self.settings.layout.tile_width == w && self.settings.layout.tile_height == h {
            return;
        }
        self.settings.layout.tile_width = w;
        self.settings.layout.tile_height = h;
        self.tile_persist = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TILE_SIZE_DEBOUNCE).await;
            let _ = this.update(cx, |state, _| state.flush_tile_persist());
        }));
    }

    /// 磁贴尺寸立即落盘（防抖到期 / 应用退出兜底）：有待写任务才写。
    fn flush_tile_persist(&mut self) {
        if self.tile_persist.take().is_some() {
            self.persist();
        }
    }

    pub fn shrink_database_memory(&self) {
        if let Err(err) = self.db.shrink_memory() {
            eprintln!("[fragmenta] 释放 SQLite 页缓存失败: {err}");
        }
    }

    /// 当前磁贴呼出热键。
    pub fn tile_hotkey(&self) -> &str {
        &self.settings.tile_hotkey
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

    /// 更新开机自启动（ticket 09）：开启时写入 HKCU Run 键，命令行
    /// `"{exe}" --silent`，每次开启按当前 exe 路径重写（便携 exe 移动后
    /// 再次开启即刷新）；关闭时删除该键。注册表操作成功后才更新内存态
    /// 并持久化 + 广播；失败返回 `Err`，内存态与 settings.json 均不变。
    pub fn set_autostart(
        &mut self,
        value: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), AutostartError> {
        if self.settings.autostart == value {
            return Ok(());
        }
        if value {
            crate::autostart::enable()?;
        } else {
            crate::autostart::disable()?;
        }
        self.settings.autostart = value;
        self.persist();
        cx.emit(SettingsEvent::Changed);
        Ok(())
    }

    /// 数据目录热迁移（ADR-0002 / 0006，ticket 06）：应用不重启，已开磁贴
    /// 后续保存自动落新库。
    ///
    /// 编排顺序：前置 `flush_all`（pending 草稿完整落库）→ 校验目标目录
    /// （空或不存在、可写、canonicalize 后非同目录非嵌套；不存在则新建）→
    /// 新路径全新连接在线备份 + 试开 → `imgs/` 随迁拷贝 → `settings.save`
    /// （失败则清理目标产物并中止，内存态零改动、旧数据完整）→ `Db` 门面
    /// 内连接热替换 + `ImageStore` 换根（store 零感知）→ 广播 `Changed` →
    /// 删除旧目录数据文件（失败仅警告不回滚，旧文件夹保留）。
    ///
    /// 同步执行于 UI 线程：编排期间无并发写入库，无后台线程与写闸门。
    pub fn set_data_dir(
        &mut self,
        new_dir: PathBuf,
        cx: &mut Context<Self>,
    ) -> Result<(), DataDirError> {
        let old_dir = self.settings.data_dir.clone();
        // 前置 flush_all：pending 草稿落旧库后再备份，迁移后新库数据完整；
        // 落库失败即中止（内容保留在 pending，可重试）
        if let Some(notes) = cx.try_global::<NotesStore>() {
            let notes = notes.0.clone();
            if notes.update(cx, |state, cx| state.flush_all(cx)) == SaveStatus::Failed {
                return Err(DataDirError::FlushFailed);
            }
        }
        // 目标目录校验（违规红字拒绝，数据不动）
        migrate::validate_target(&old_dir, &new_dir)?;
        // 在线备份到新路径全新连接并试开验证（失败清理目标产物）
        let staged = match self.db.stage_migration(&new_dir.join("fragmenta.db")) {
            Ok(staged) => staged,
            Err(err) => {
                let _ = migrate::remove_data_products(&new_dir);
                return Err(DataDirError::Backup(err));
            }
        };
        // imgs 图库随迁（失败先关暂存连接再清理目标产物，Windows 文件占用）
        if let Err(err) = migrate::copy_imgs(&old_dir, &new_dir) {
            drop(staged);
            let _ = migrate::remove_data_products(&new_dir);
            return Err(err);
        }
        // settings.save：失败清理目标产物并中止（内存态零改动、旧数据完整）
        self.settings.data_dir = new_dir.clone();
        if let Err(err) = self.settings.save(&self.path) {
            self.settings.data_dir = old_dir;
            drop(staged);
            let _ = migrate::remove_data_products(&new_dir);
            return Err(DataDirError::Save(err));
        }
        // 连接热替换 + 图库换根：全应用共享 Arc<Db> 与 ImageStore 即时生效
        self.db.swap_connection(staged);
        cx.set_global(ImageStore(fragmenta_core::media::Images::new(
            new_dir.join("imgs"),
        )));
        cx.emit(SettingsEvent::Changed);
        // 收尾：删除旧目录数据文件（旧文件夹及无关文件保留）；
        // 删除失败仅警告，已提交的迁移不回滚
        if let Err(err) = migrate::remove_data_products(&old_dir) {
            eprintln!("[fragmenta] 迁移后清理旧数据文件失败（可手动清理）: {err}");
        }
        Ok(())
    }

    /// 改绑磁贴呼出热键：先校验格式（纯修饰键等无效组合在解析层被拒），
    /// 再经热键控制器注册（被其他程序占用时返回 `Err`，旧热键保持可用），
    /// 成功后持久化 + 广播。返回 `Err` 时不落库。
    ///
    /// 热键控制器缺席（启动期注册失败的降级模式）时跳过注册直接落库，
    /// 重启后新热键生效。
    pub fn set_hotkey(&mut self, hotkey: &str, cx: &mut Context<Self>) -> Result<(), HotkeyError> {
        hotkey.parse::<HotKey>().map_err(HotkeyError::Parse)?;
        if self.settings.tile_hotkey == hotkey {
            return Ok(());
        }
        if let Some(controller) = cx.try_global::<HotkeyController>() {
            controller.rebind(hotkey)?;
        }
        self.settings.tile_hotkey = hotkey.to_string();
        self.persist();
        cx.emit(SettingsEvent::Changed);
        Ok(())
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
            theme::apply(mode, cx);
        }
    }

    /// 按设置加载内置 JSON；Auto 解析为当前系统外观。
    fn apply_theme_mode(&mut self, cx: &mut Context<Self>) {
        let mode = match self.settings.theme {
            ThemeMode::Light => KitThemeMode::Light,
            ThemeMode::Dark => KitThemeMode::Dark,
            ThemeMode::Auto => kit_mode_of(cx.window_appearance()),
        };
        theme::apply(mode, cx);
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{AppContext as _, TestAppContext, rgb};

    /// 磁贴尺寸记忆：内存即时生效，落盘按防抖窗口合并；到期后新进程
    /// 从 settings.json 读到调整后的尺寸。
    #[gpui_kit::test]
    fn tile_size_persists_after_debounce_and_reloads(cx: &mut TestAppContext) {
        cx.update(gpui_kit::component::theme::init);
        let path = PathBuf::from("temp/tile-size-settings.json");
        let _ = std::fs::remove_file(&path);
        let db = Arc::new(Db::open_in_memory().unwrap());
        let state = cx.new(|cx| SettingsState::new(Settings::default(), path.clone(), db, cx));
        state.update(cx, |state, cx| {
            // 默认值即磁贴初始尺寸
            assert_eq!(state.tile_size(), (px(360.), px(480.)));
            state.set_tile_size(px(420.), px(560.), cx);
            // 同值提交（拖动 / scale 变化不改逻辑尺寸）跳过
            state.set_tile_size(px(420.), px(560.), cx);
            assert_eq!(state.tile_size(), (px(420.), px(560.)));
        });
        // 防抖未到期：磁盘仍为默认值
        assert_eq!(Settings::load(&path).unwrap().layout.tile_width, 360.);
        cx.executor().run_until_parked();
        cx.executor().advance_clock(TILE_SIZE_DEBOUNCE);
        cx.executor().run_until_parked();
        // 到期落盘：重载（新进程）读到记忆尺寸
        let reloaded = Settings::load(&path).unwrap();
        assert_eq!(reloaded.layout.tile_width, 420.);
        assert_eq!(reloaded.layout.tile_height, 560.);
        let _ = std::fs::remove_file(&path);
    }

    /// 系统外观往返切换始终加载应用 JSON，显式模式不跟随系统变化。
    #[gpui_kit::test]
    fn auto_bridge_uses_json_and_respects_explicit_mode(cx: &mut TestAppContext) {
        cx.update(gpui_kit::component::theme::init);
        let db = Arc::new(Db::open_in_memory().unwrap());
        let state = cx.new(|cx| {
            SettingsState::new(
                Settings {
                    theme: ThemeMode::Auto,
                    ..Default::default()
                },
                PathBuf::new(),
                db,
                cx,
            )
        });
        state.update(cx, |state, cx| {
            state.sync_auto_mode(WindowAppearance::Dark, cx);
            assert_eq!(cx.theme().mode, KitThemeMode::Dark);
            assert_eq!(cx.theme().dark_theme.name.as_ref(), "Fragmenta Dark");
            state.sync_auto_mode(WindowAppearance::Light, cx);
            assert_eq!(cx.theme().mode, KitThemeMode::Light);
            assert_eq!(cx.theme().primary, rgb(0x0F6CBD).into());
            state.settings.theme = ThemeMode::Light;
            state.sync_auto_mode(WindowAppearance::Dark, cx);
            assert_eq!(cx.theme().mode, KitThemeMode::Light);
        });
    }
}
