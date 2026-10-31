//! Schema 迁移：`PRAGMA user_version` 驱动，建库在单事务内原子完成（架构 §6）。

use super::{DbError, Result};
use rusqlite::Connection;

/// 当前程序支持的 Schema 版本。
pub const SCHEMA_VERSION: i64 = 1;

/// v1 全量 schema（DDL 按外键依赖排序，父表在前）。
/// 项目未发布：分类移除后的终态直接作为 v1，不保留过渡迁移。
const V1: &str = "
CREATE TABLE tags (
  id   INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE,
  color TEXT                                    -- #RRGGBB；NULL = 未选色
);
CREATE TABLE notes (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  title       TEXT NOT NULL DEFAULT '',
  content     TEXT NOT NULL DEFAULT '',        -- Markdown 源文
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
  content    TEXT NOT NULL,
  date       TEXT NOT NULL,                    -- YYYY-MM-DD
  start_min  INTEGER,                         -- 自午夜分钟数 0–1439；与 end_min 同 NULL 或同非 NULL
  end_min    INTEGER,
  done       INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_notes_updated ON notes(updated_at DESC);
CREATE INDEX idx_todos_date ON todos(date);
CREATE INDEX idx_note_tags_tag ON note_tags(tag_id);
";

/// 读取文件版本并按需建库；版本过新直接报错，库文件保持不动。
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
    tx.execute_batch(V1)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}
