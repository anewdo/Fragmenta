//! db:SQLite 全部 SQL 与迁移的唯一归属
//!
//! state 层私有持有 `Arc<Db>`，页面禁止 import 本模块；
//! 本 crate 零 gpui 依赖，数据逻辑可独立编译与测试。

mod migrate;
mod notes;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

/// db 层错误。
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// 数据库文件由更新版本的程序创建，本版本无法打开。
    #[error("数据库版本过新：文件 schema v{found}，程序支持 v{expected}")]
    UnsupportedVersion { found: i64, expected: i64 },
    /// 更新的目标笔记不存在。
    #[error("笔记 {0} 不存在")]
    NoteNotFound(i64),
    /// 库中时间戳不符合 RFC3339。
    #[error("库中时间戳损坏：{0}")]
    CorruptTimestamp(String),
    /// 文件系统错误（建目录 / 打开文件）。
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
    /// SQLite 错误。
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, DbError>;

/// SQLite 门面：内部 `Mutex<Connection>`，全部方法串行化访问（架构 D2，无全局静态）。
pub struct Db {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db").finish_non_exhaustive()
    }
}

impl Db {
    /// 打开（或创建）数据库文件并执行迁移；父目录不存在时自动创建。
    ///
    /// 默认调用路径为 `Settings.data_dir / fragmenta.db`（data_dir 默认 exe 同级 `data/`）。
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut conn = Connection::open(path)?;
        Self::init(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 内存库，测试入口。
    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        Self::init(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 连接级初始化：外键约束 + 版本迁移。
    fn init(conn: &mut Connection) -> Result<()> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate::run(conn)
    }
}
