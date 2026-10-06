//! 图片图库：内容哈希命名去重与 `imgs/` 引用解析（架构 §5.4 / §8 / D6）。
//!
//! 图库物理目录由构造注入（应用层固定传 `{exe_dir}/imgs`，不随 data_dir 迁移变化）；
//! 正文中的引用统一为 `imgs/{name}` 逻辑前缀，解析规则与渲染插件（架构 §8）一致。

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// media 层错误。
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MediaError>;

/// 图片图库（架构 D6：`imgs/` 固定于 exe 旁，与 data_dir 无关）。
#[derive(Debug, Clone)]
pub struct Images {
    base: PathBuf,
}

impl Images {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    /// 导入图片字节：sha256 内容哈希命名，同名文件已存在则复用；
    /// 返回正文相对引用 `imgs/{hash}.{ext}`。
    ///
    /// 扩展名做规范化（去前导句点、转小写），空扩展落为无扩展名文件。
    pub fn import(&self, bytes: &[u8], ext: &str) -> Result<String> {
        let hex: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        let name = if ext.is_empty() {
            hex
        } else {
            format!("{hex}.{ext}")
        };
        let path = self.base.join(&name);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, bytes)?;
        }
        Ok(format!("imgs/{name}"))
    }

    /// 解析正文引用为可加载路径（架构 §8 三条规则，与渲染插件语义一致）：
    ///
    /// - `imgs/..` → 图库目录下的绝对路径
    /// - 绝对路径 → 原样返回
    /// - 其余（越界相对路径 / 协议 URL 等）→ `None`
    ///
    /// 只做路径映射，不检查文件存在（缺失由调用方回退处理）。
    pub fn resolve(&self, ref_path: &str) -> Option<PathBuf> {
        if let Some(rest) = ref_path.strip_prefix("imgs/") {
            Some(self.base.join(rest))
        } else if Path::new(ref_path).is_absolute() {
            Some(PathBuf::from(ref_path))
        } else {
            None
        }
    }
}
