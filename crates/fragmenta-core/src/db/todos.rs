//! 待办数据访问（架构 §5.2）：upsert / delete / 闭区间查询。

use chrono::{Local, NaiveDate};
use rusqlite::{Connection, params};

use super::{Db, DbError, Result, date_str, parse_date, parse_timestamp};
use crate::model::Todo;

impl Db {
    /// 插入或更新待办：`id == 0` 视为新待办插入，否则按 id 更新
    /// （标题 / 详情 / 日期 / 完成态均可修改）。
    ///
    /// 回填 id 与 created_at / updated_at（以库中数据为准）；更新不覆盖 created_at。
    pub fn upsert_todo(&self, todo: &mut Todo) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Local::now().to_rfc3339();
        let done = todo.done as i64;
        if todo.id == 0 {
            conn.execute(
                "INSERT INTO todos (title, detail, date, done, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![todo.title, todo.detail, date_str(todo.date), done, now],
            )?;
            todo.id = conn.last_insert_rowid();
        } else {
            let changed = conn.execute(
                "UPDATE todos SET title = ?1, detail = ?2, date = ?3, done = ?4, updated_at = ?5
                 WHERE id = ?6",
                params![
                    todo.title,
                    todo.detail,
                    date_str(todo.date),
                    done,
                    now,
                    todo.id
                ],
            )?;
            if changed == 0 {
                return Err(DbError::TodoNotFound(todo.id));
            }
        }
        read_back_timestamps(&conn, todo)?;
        Ok(())
    }

    /// 删除待办；目标不存在时为无操作。
    pub fn delete_todo(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM todos WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 查询闭区间 [from, to] 内的待办（date 升序，同日按 id 升序）。
    pub fn todos_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<Todo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, detail, date, done, created_at, updated_at
             FROM todos
             WHERE date BETWEEN ?1 AND ?2
             ORDER BY date ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![date_str(from), date_str(to)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        let mut todos = Vec::new();
        for row in rows {
            let (id, title, detail, date, done, created, updated) = row?;
            todos.push(Todo {
                id,
                title,
                detail,
                date: parse_date(&date)?,
                done: done != 0,
                created_at: parse_timestamp(&created)?,
                updated_at: parse_timestamp(&updated)?,
            });
        }
        Ok(todos)
    }
}

/// 从库中回填待办时间戳（与笔记 upsert 同构）。
fn read_back_timestamps(conn: &Connection, todo: &mut Todo) -> Result<()> {
    let (created, updated): (String, String) = conn.query_row(
        "SELECT created_at, updated_at FROM todos WHERE id = ?1",
        params![todo.id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    todo.created_at = parse_timestamp(&created)?;
    todo.updated_at = parse_timestamp(&updated)?;
    Ok(())
}
