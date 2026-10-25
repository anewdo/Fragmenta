//! 测试公共工具（仅被各测试文件 `mod common;` 引入，不产生独立测试目标）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 测试用临时目录：位于仓库 `temp/tests/` 下（经 `CARGO_MANIFEST_DIR` 推导），
/// 名称含进程号与自增序号保证唯一；析构时递归删除。
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../temp/tests")
            .join(format!("{name}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path).expect("创建测试临时目录失败");
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
