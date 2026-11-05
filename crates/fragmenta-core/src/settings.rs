use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 主题选择：四套内置配色 + Auto（跟随系统深浅色）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
    /// 文艺绿：浅色系，纸墨绿调。
    LiteraryGreen,
    /// 陶瓷：浅色系，暖陶土调。
    Ceramic,
    Auto,
}

/// 内容正文字号的可调范围（逻辑像素）。
pub const FONT_SIZE_MIN_PX: f32 = 12.;
pub const FONT_SIZE_MAX_PX: f32 = 24.;
/// 内容正文字号默认值（逻辑像素）。
pub const FONT_SIZE_DEFAULT_PX: f32 = 16.;
/// 出厂默认阅读字体：思源黑体。
///
/// 设置文件保存出厂族名，应用层启动时按本机安装情况解析族名别名；未安装
/// 时本次运行回落系统默认，之后安装字体会在下次启动生效。
pub const DEFAULT_FONT_FAMILY: &str = "Source Han Sans SC";
/// 行距倍数的可调范围与默认值（相对字号）。
pub const LINE_HEIGHT_MIN: f32 = 1.2;
pub const LINE_HEIGHT_MAX: f32 = 2.0;
pub const LINE_HEIGHT_DEFAULT: f32 = 1.6;
/// 笔记列表默认宽度（逻辑像素）；笔记页搜索栏与高级筛选面板沿用该宽度。
pub const NOTES_LIST_WIDTH_DEFAULT_PX: f32 = 320.;

/// 应用设置，持久化为 exe 旁 settings.json。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    /// 编辑器与主窗口列表的正文字号（逻辑像素）。
    pub editor_font_px: f32,
    /// 编辑器标题与正文的字体族；`None` 跟随系统默认。
    ///
    /// 应用层只经 `FontSettings` 写入该字段，以同步已解析的字体族缓存。
    pub editor_font_family: Option<String>,
    /// 编辑器标题与正文的行距倍数。
    pub editor_line_height: f32,
    /// 从系统字体补充到字体下拉的自定义字体族（按添加顺序、去重）；
    /// 内置候选之外的用户选择，两份下拉共享。
    pub custom_font_families: Vec<String>,
    /// 磁贴小窗的标题 / 正文字号（逻辑像素）。
    pub tile_font_px: f32,
    /// 磁贴小窗标题与正文的字体族；`None` 跟随系统默认。
    ///
    /// 应用层只经 `FontSettings` 写入该字段，以同步已解析的字体族缓存。
    pub tile_font_family: Option<String>,
    /// 磁贴小窗标题与正文的行距倍数。
    pub tile_line_height: f32,
    /// 数据目录（数据库与图片图库平铺于其内），默认 {exe_dir}/data，可迁移。
    pub data_dir: PathBuf,
    /// 磁贴呼出热键，如 "ctrl+alt+n"。
    pub tile_hotkey: String,
    /// 关闭主窗口时直接退出；默认 false（托盘驻留，架构 §5.8）。
    pub close_to_quit: bool,
    /// 开机自启动；默认 false。应用层在开关开启时写入 HKCU Run 键
    pub autostart: bool,
    pub layout: LayoutSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutSettings {
    pub sidebar_width: f32,
    pub notes_list_width: f32,
    pub editor_source_width: f32,
    /// 磁贴窗口尺寸（逻辑像素）：呼出新磁贴按此尺寸打开，用户调整磁贴
    /// 后更新；默认值即磁贴初始尺寸。
    pub tile_width: f32,
    pub tile_height: f32,
    /// 主窗口上次关闭时的左上角（逻辑像素）：下次打开沿用；`None` 时居中。
    pub main_window_origin: Option<WindowOrigin>,
}

/// 窗口左上角（逻辑像素），仅用于持久化窗口位置。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowOrigin {
    pub x: f32,
    pub y: f32,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            sidebar_width: 255.,
            notes_list_width: NOTES_LIST_WIDTH_DEFAULT_PX,
            editor_source_width: 360.,
            tile_width: 300.,
            tile_height: 360.,
            main_window_origin: None,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::default(),
            editor_font_px: FONT_SIZE_DEFAULT_PX,
            editor_font_family: Some(DEFAULT_FONT_FAMILY.into()),
            editor_line_height: LINE_HEIGHT_DEFAULT,
            custom_font_families: Vec::new(),
            tile_font_px: FONT_SIZE_DEFAULT_PX,
            tile_font_family: Some(DEFAULT_FONT_FAMILY.into()),
            tile_line_height: LINE_HEIGHT_DEFAULT,
            data_dir: default_data_dir(),
            tile_hotkey: default_tile_hotkey(),
            close_to_quit: false,
            autostart: false,
            layout: LayoutSettings::default(),
        }
    }
}

/// 字号钳制到可调范围；损坏值回落默认（settings.json 可被手动编辑）。
pub fn clamp_font_px(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(FONT_SIZE_MIN_PX, FONT_SIZE_MAX_PX)
    } else {
        FONT_SIZE_DEFAULT_PX
    }
}

/// 行距钳制到可调范围并按 0.05 步进取整；损坏值回落默认
/// （settings.json 可被手动编辑，滑杆拖动也避免浮点抖动写盘）。
pub fn clamp_line_height(value: f32) -> f32 {
    if value.is_finite() {
        (value.clamp(LINE_HEIGHT_MIN, LINE_HEIGHT_MAX) * 20.).round() / 20.
    } else {
        LINE_HEIGHT_DEFAULT
    }
}

/// 字体族规范化：去首尾空白，空串视为跟随系统默认。
pub fn normalize_font_family(family: Option<String>) -> Option<String> {
    family
        .map(|family| family.trim().to_string())
        .filter(|family| !family.is_empty())
}

/// 自定义字体族列表规范化：去首尾空白、丢弃空值，按 ASCII 大小写不敏感去重
///（settings.json 可被手动编辑，重复项会让下拉出现同名选项）。
pub fn normalize_font_families(families: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::with_capacity(families.len());
    for family in families {
        let family = family.trim();
        if !family.is_empty() && !kept.iter().any(|name| name.eq_ignore_ascii_case(family)) {
            kept.push(family.to_string());
        }
    }
    kept
}

/// 默认数据目录：{exe_dir}/data。
fn default_data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("data")
}

fn default_tile_hotkey() -> String {
    "ctrl+alt+n".to_string()
}

/// settings.json 读写错误。
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("读取设置文件失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("解析设置文件失败: {0}")]
    Parse(#[from] serde_json::Error),
}

impl Settings {
    /// 从 JSON 文件加载；文件不存在返回默认值。
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mut settings: Self = serde_json::from_str(&text)?;
                settings.editor_font_px = clamp_font_px(settings.editor_font_px);
                settings.tile_font_px = clamp_font_px(settings.tile_font_px);
                settings.editor_font_family = normalize_font_family(settings.editor_font_family);
                settings.tile_font_family = normalize_font_family(settings.tile_font_family);
                settings.custom_font_families =
                    normalize_font_families(std::mem::take(&mut settings.custom_font_families));
                settings.editor_line_height = clamp_line_height(settings.editor_line_height);
                settings.tile_line_height = clamp_line_height(settings.tile_line_height);
                Ok(settings)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// 保存为 JSON 文件（pretty）。
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }
}
