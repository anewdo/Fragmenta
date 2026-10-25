//! 迁移黑盒测试：经公共 API（`Db::open`）验证 schema 版本推进（真实 SQLite）。
//!
//! 项目未发布：todos 表重定义经一次性原地迁移收敛回 v1（版本号不升 v2），存量库
//! 已迁移完毕，过渡迁移代码随之删除。终态覆盖：全新库建 v1 全量、重开幂等、版本过新拒绝。

mod common;

use chrono::NaiveDate;
use common::{TempDir, raw_conn};
use fragmenta_core::db::{Db, DbError};

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// 全新库建 v1 全量 schema：todos 表可写可读（含区间），重开幂等。
#[test]
fn fresh_db_builds_v1_and_reopens() {
    let dir = TempDir::new("db-migrate-fresh");
    let db_path = dir.path().join("fresh.db");

    let db = Db::open(&db_path).unwrap();
    let mut todo = fragmenta_core::model::Todo {
        id: 0,
        content: "上午会".into(),
        date: date(2026, 10, 11),
        interval: Some((
            chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
        )),
        done: false,
        created_at: chrono::Local::now(),
        updated_at: chrono::Local::now(),
    };
    db.upsert_todo(&mut todo).unwrap();
    let version: i64 = raw_conn(&db_path)
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);

    // 重开幂等：版本已到位、结构已是新表，不再触发任何迁移，数据保持不变
    drop(db);
    let db = Db::open(&db_path).unwrap();
    let todos = db
        .todos_between(date(2026, 10, 11), date(2026, 10, 11))
        .unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].content, "上午会");
    assert_eq!(
        todos[0].interval,
        Some((
            chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
        ))
    );
}

/// 版本过新的库直接拒绝打开，库文件保持不动。
#[test]
fn rejects_newer_version() {
    let dir = TempDir::new("db-migrate-newer");
    let db_path = dir.path().join("newer.db");
    {
        let conn = raw_conn(&db_path);
        conn.pragma_update(None, "user_version", 99).unwrap();
    }
    let err = Db::open(&db_path).unwrap_err();
    assert!(matches!(
        err,
        DbError::UnsupportedVersion {
            found: 99,
            expected: 1
        }
    ));
    // 库文件保持不动：版本仍为 99
    let version: i64 = raw_conn(&db_path)
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 99);
}
