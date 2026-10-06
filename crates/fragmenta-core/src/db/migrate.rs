//! Schema 迁移：`PRAGMA user_version` 驱动，版本推进在单事务内原子完成（架构 §6）。

use super::{DbError, Result};
use rusqlite::Connection;

/// 当前程序支持的 Schema 版本。
pub const SCHEMA_VERSION: i64 = 1;

/// v1 全量 schema（架构 §6 定案；DDL 按外键依赖排序，父表在前）。
const V1: &str = "
CREATE TABLE categories (
  id   INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);
CREATE TABLE tags (
  id   INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);
CREATE TABLE notes (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  title       TEXT NOT NULL DEFAULT '',
  content     TEXT NOT NULL DEFAULT '',        -- Markdown 源文
  category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
  created_at  TEXT NOT NULL,                   -- RFC3339 本地时间
  updated_at  TEXT NOT NULL
);
CREATE TABLE note_tags (
  note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  tag_id  INTEGER NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
  PRIMARY KEY (note_id, tag_id)
);
CREATE TABLE todos (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  title      TEXT NOT NULL,
  detail     TEXT NOT NULL DEFAULT '',
  date       TEXT NOT NULL,                    -- YYYY-MM-DD
  done       INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_notes_updated ON notes(updated_at DESC);
CREATE INDEX idx_todos_date ON todos(date);
";

/// 读取文件版本并按需推进迁移；版本过新直接报错，库文件保持不动。
pub(super) fn run(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(DbError::UnsupportedVersion {
            found: current,
            expected: SCHEMA_VERSION,
        });
    }
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    let tx = conn.transaction()?;
    // 版本逐级推进（当前仅有 v1，直接建全量）
    if current < 1 {
        tx.execute_batch(V1)?;
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}
