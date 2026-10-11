//! 待办数据访问（架构 §5.2）：upsert / delete / 闭区间查询。

use chrono::{Local, NaiveDate, NaiveTime, Timelike};
use rusqlite::{Connection, params};

use super::{Db, DbError, Result, date_str, parse_date, parse_timestamp};
use crate::model::Todo;

impl Db {
    /// 插入或更新待办：`id == 0` 视为新待办插入，否则按 id 更新
    /// （内容 / 日期 / 区间 / 完成态均可修改）。
    ///
    /// 回填 id 与 created_at / updated_at（以库中数据为准）；更新不覆盖 created_at。
    pub fn upsert_todo(&self, todo: &mut Todo) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Local::now().to_rfc3339();
        let done = todo.done as i64;
        // 区间两列同进同出：tuple 类型保证同 NULL 或同非 NULL
        let (start_min, end_min) = match todo.interval {
            None => (None, None),
            Some((start, end)) => (Some(time_to_min(start)), Some(time_to_min(end))),
        };
        if todo.id == 0 {
            conn.execute(
                "INSERT INTO todos (content, date, start_min, end_min, done, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![todo.content, date_str(todo.date), start_min, end_min, done, now],
            )?;
            todo.id = conn.last_insert_rowid();
        } else {
            let changed = conn.execute(
                "UPDATE todos SET content = ?1, date = ?2, start_min = ?3, end_min = ?4,
                   done = ?5, updated_at = ?6 WHERE id = ?7",
                params![
                    todo.content,
                    date_str(todo.date),
                    start_min,
                    end_min,
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

    /// 查询闭区间 [from, to] 内的待办。
    ///
    /// 排序：date 升序 → start_min 升序（SQLite NULL 排最前，全天置顶天然成立）
    /// → id 升序；done 不参与排序（完成态保持原位置）。
    pub fn todos_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<Todo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, content, date, start_min, end_min, done, created_at, updated_at
             FROM todos
             WHERE date BETWEEN ?1 AND ?2
             ORDER BY date ASC, start_min ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![date_str(from), date_str(to)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;
        let mut todos = Vec::new();
        for row in rows {
            let (id, content, date, start_min, end_min, done, created, updated) = row?;
            todos.push(Todo {
                id,
                content,
                date: parse_date(&date)?,
                interval: read_interval(id, start_min, end_min)?,
                done: done != 0,
                created_at: parse_timestamp(&created)?,
                updated_at: parse_timestamp(&updated)?,
            });
        }
        Ok(todos)
    }
}

/// 库中区间两列 → 模型区间：同 NULL 为全天，同值对还原区间；
/// 部分空置或超出当日分钟范围（0–1439）视为损坏，公共 API 边界拒绝。
fn read_interval(
    id: i64,
    start_min: Option<i64>,
    end_min: Option<i64>,
) -> Result<Option<(NaiveTime, NaiveTime)>> {
    match (start_min, end_min) {
        (None, None) => Ok(None),
        (Some(start), Some(end)) => match (min_to_time(start), min_to_time(end)) {
            (Some(start), Some(end)) => Ok(Some((start, end))),
            _ => Err(DbError::CorruptInterval(id)),
        },
        _ => Err(DbError::CorruptInterval(id)),
    }
}

/// NaiveTime → 自午夜分钟数（TimeField 为分钟精度，秒恒为 0）。
fn time_to_min(t: NaiveTime) -> i64 {
    t.hour() as i64 * 60 + t.minute() as i64
}

/// 自午夜分钟数 → NaiveTime；越界（负数 / ≥ 1440）返回 None。
fn min_to_time(min: i64) -> Option<NaiveTime> {
    NaiveTime::from_num_seconds_from_midnight_opt((min * 60) as u32, 0)
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
