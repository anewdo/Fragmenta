//! db:SQLite 全部 SQL 与迁移的唯一归属
//!
//! state 层私有持有 `Arc<Db>`，页面禁止 import 本模块；
//! 本 crate 零 gpui 依赖，数据逻辑可独立编译与测试。

mod migrate;
mod notes;
mod taxonomy;
mod todos;

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate};
use rusqlite::{Connection, params};

use crate::model::DayActivity;

/// db 层错误。
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// 数据库文件由更新版本的程序创建，本版本无法打开。
    #[error("数据库版本过新：文件 schema v{found}，程序支持 v{expected}")]
    UnsupportedVersion { found: i64, expected: i64 },
    /// 操作的目标笔记不存在。
    #[error("笔记 {0} 不存在")]
    NoteNotFound(i64),
    /// 操作的目标待办不存在。
    #[error("待办 {0} 不存在")]
    TodoNotFound(i64),
    /// 操作的目标分类不存在。
    #[error("分类 {0} 不存在")]
    CategoryNotFound(String),
    /// 重命名的目标分类名已被占用。
    #[error("分类 {0} 已存在")]
    CategoryExists(String),
    /// 库中时间戳不符合 RFC3339。
    #[error("库中时间戳损坏：{0}")]
    CorruptTimestamp(String),
    /// 库中日期不符合 YYYY-MM-DD。
    #[error("库中日期损坏：{0}")]
    CorruptDate(String),
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
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
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

    /// 首页日历聚合：区间内有活动的日子，按日期升序。
    ///
    /// 笔记按 created_at 归日、待办按 date 归日；仅返回有活动的日子，
    /// 无活动日不产生条目（日历渲染以缺失 = 无指示）。
    pub fn activity_by_day(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<DayActivity>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT date, SUM(n), SUM(t) FROM (
               SELECT substr(created_at, 1, 10) AS date, COUNT(*) AS n, 0 AS t
                 FROM notes
                WHERE substr(created_at, 1, 10) BETWEEN ?1 AND ?2
                GROUP BY date
               UNION ALL
               SELECT date, 0, COUNT(*) AS t
                 FROM todos
                WHERE date BETWEEN ?1 AND ?2
                GROUP BY date
             ) GROUP BY date ORDER BY date",
        )?;
        let rows = stmt.query_map(params![date_str(from), date_str(to)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut days = Vec::new();
        for row in rows {
            let (date, note_count, todo_count) = row?;
            days.push(DayActivity {
                date: parse_date(&date)?,
                note_count,
                todo_count,
            });
        }
        Ok(days)
    }

    /// 在线一致备份到 dst（ADR-0002：数据迁移走 backup API，禁止文件系统裸拷）。
    ///
    /// dst 须为另一个 `Db` 实例；备份不中断源库读写（journal 模式无关）。
    pub fn backup_to(&self, dst: &Db) -> Result<()> {
        let src = self.conn.lock().unwrap();
        let mut dst_conn = dst.conn.lock().unwrap();
        let backup = rusqlite::backup::Backup::new(&src, &mut dst_conn)?;
        backup.run_to_completion(128, Duration::from_millis(1), None)?;
        Ok(())
    }

    /// 连接级初始化：外键约束 + 版本迁移。
    fn init(conn: &mut Connection) -> Result<()> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate::run(conn)
    }
}

/// 解析库中 RFC3339 本地时间。
pub(super) fn parse_timestamp(s: &str) -> Result<DateTime<Local>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Local))
        .map_err(|_| DbError::CorruptTimestamp(s.to_string()))
}

/// 解析库中 YYYY-MM-DD 日期。
pub(super) fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| DbError::CorruptDate(s.to_string()))
}

/// NaiveDate → 库中文本格式 YYYY-MM-DD。
pub(super) fn date_str(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}
