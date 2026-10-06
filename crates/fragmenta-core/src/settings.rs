//! settings.json 读写与路径默认值（架构 §5.3 / D6）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 主题模式；Auto 的 Windows 行为待 Phase 2 spike 裁决（架构 §5.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
    Auto,
}

/// 字号档位，映射 Theme.font_size。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FontScale {
    Small,
    #[default]
    Medium,
    Large,
}

/// 应用设置，持久化为 exe 旁 settings.json。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub font_scale: FontScale,
    /// 数据目录（数据库所在），默认 {exe_dir}/data，可迁移。
    pub data_dir: PathBuf,
    /// 磁贴呼出热键，如 "ctrl+alt+n"。
    pub tile_hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::default(),
            font_scale: FontScale::default(),
            data_dir: default_data_dir(),
            tile_hotkey: default_tile_hotkey(),
        }
    }
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
            Ok(text) => Ok(serde_json::from_str(&text)?),
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
