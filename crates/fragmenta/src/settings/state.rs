//! 字号档位不写全局主题：kit 组件几何按 16px rem 基准设计，全局放大
//! 会使 px 固定几何组件（日历等）文字溢出。字号只作用于内容阅读面，
//! 由各内容组件经 [`SettingsState::content_font_px`] 显式应用。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fragmenta_core::db::Db;
use fragmenta_core::settings::{FontScale, Settings, ThemeMode};
use global_hotkey::hotkey::HotKey;
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode as KitThemeMode};
use gpui_kit::{
    Context, Entity, EventEmitter, Global, Pixels, Subscription, Window, WindowAppearance, px,
};

use crate::editor::ImageStore;
use crate::hotkey::{HotkeyController, HotkeyError};
use crate::notes::{NotesStore, SaveStatus};
use crate::settings::migrate::{self, DataDirError};
use crate::settings::theme_schemes::{self, ThemeScheme};

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
}

impl SettingsState {
    /// 构造即按当前设置应用主题模式（启动期主题应用唯一入口）。
    pub fn new(settings: Settings, path: PathBuf, db: Arc<Db>, cx: &mut Context<Self>) -> Self {
        let mut state = Self { settings, path, db };
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
            Theme::change(mode, None, cx);
        }
    }

    /// 按设置应用主题模式：内置色系 = Light 基线 + 核心 token 覆盖
    /// （ADR-0005）；Auto 解析为当前系统外观（App 级，无窗口期同样可解析）。
    fn apply_theme_mode(&mut self, cx: &mut Context<Self>) {
        match self.settings.theme {
            ThemeMode::Light => Theme::change(KitThemeMode::Light, None, cx),
            ThemeMode::Dark => Theme::change(KitThemeMode::Dark, None, cx),
            ThemeMode::Auto => Theme::change(kit_mode_of(cx.window_appearance()), None, cx),
            ThemeMode::DouShaGreen => Self::apply_color_scheme(theme_schemes::dou_sha_green(), cx),
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
