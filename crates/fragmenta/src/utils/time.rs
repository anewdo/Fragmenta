//! 时间与日期的展示文案：相对时间、待办区间、日期与日历表头。
//!
//! 仅承载"时间 → 文案"的格式化；取当前时间、日期运算与持久化留在各功能模块。

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, Weekday};

/// 笔记创建时间的相对文案：一周内按刚刚 / 分钟 / 小时 / 天，更早显示日期。
pub(crate) fn relative_created_at(created_at: DateTime<Local>) -> String {
    relative_at(created_at, Local::now())
}

fn relative_at(created_at: DateTime<Local>, now: DateTime<Local>) -> String {
    let seconds = now.signed_duration_since(created_at).num_seconds();
    if seconds < 60 {
        "刚刚".to_string()
    } else if seconds < 60 * 60 {
        format!("{} 分钟前", seconds / 60)
    } else if seconds < 60 * 60 * 24 {
        format!("{} 小时前", seconds / 60 / 60)
    } else if seconds < 60 * 60 * 24 * 7 {
        format!("{} 天前", seconds / 60 / 60 / 24)
    } else {
        created_at.format("%Y-%m-%d").to_string()
    }
}

/// 待办区间文案：`09:00 至 10:00`。
pub(crate) fn todo_interval_label(start: NaiveTime, end: NaiveTime) -> String {
    format!("{} 至 {}", start.format("%H:%M"), end.format("%H:%M"))
}

/// 日期文案：`10月11日 周六`。
pub(crate) fn date_label(date: NaiveDate) -> String {
    format!(
        "{}月{}日 {}",
        date.month(),
        date.day(),
        weekday_label(date.weekday())
    )
}

/// 星期全称：`周一` … `周日`。
fn weekday_label(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "周一",
        Weekday::Tue => "周二",
        Weekday::Wed => "周三",
        Weekday::Thu => "周四",
        Weekday::Fri => "周五",
        Weekday::Sat => "周六",
        Weekday::Sun => "周日",
    }
}

/// 日历表头星期缩写（0 = 周日 … 6 = 周六，kit 日历传值约定）。
pub(crate) fn calendar_weekday_label(value: i32) -> &'static str {
    match value {
        0 => "日",
        1 => "一",
        2 => "二",
        3 => "三",
        4 => "四",
        5 => "五",
        _ => "六",
    }
}

/// 日历月标题：`10月`。
pub(crate) fn month_label(month: i32) -> String {
    format!("{month}月")
}

#[cfg(test)]
mod tests {
    use super::{
        calendar_weekday_label, date_label, month_label, relative_at, todo_interval_label,
    };
    use chrono::{Duration, Local, NaiveDate, NaiveTime};

    #[test]
    fn relative_time_uses_readable_buckets() {
        let now = Local::now();
        assert_eq!(relative_at(now - Duration::seconds(5), now), "刚刚");
        assert_eq!(relative_at(now - Duration::minutes(5), now), "5 分钟前");
        assert_eq!(relative_at(now - Duration::hours(3), now), "3 小时前");
        assert_eq!(relative_at(now - Duration::days(3), now), "3 天前");
        let old = now - Duration::days(30);
        assert_eq!(relative_at(old, now), old.format("%Y-%m-%d").to_string());
    }

    #[test]
    fn todo_interval_and_date_labels() {
        let start = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        let end = NaiveTime::from_hms_opt(10, 30, 0).unwrap();
        assert_eq!(todo_interval_label(start, end), "09:00 至 10:30");

        let date = NaiveDate::from_ymd_opt(2026, 10, 11).unwrap();
        assert_eq!(date_label(date), "10月11日 周日");
        assert_eq!(month_label(10), "10月");
        assert_eq!(calendar_weekday_label(0), "日");
        assert_eq!(calendar_weekday_label(1), "一");
        assert_eq!(calendar_weekday_label(6), "六");
    }
}
