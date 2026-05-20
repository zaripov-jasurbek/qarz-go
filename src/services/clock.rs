//! Тонкая абстракция над "локальным временем пользователя".
//!
//! Мы не вытаскиваем таймзону из Telegram (Bot API её не отдаёт), а ставим
//! фиксированный сдвиг UTC+5 — это столичная зона Узбекистана/Казахстана и
//! плюс-минус подходит для основной аудитории. Если потом захочется
//! персональных зон — заменим эту функцию на чтение `User.timezone`.

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime, TimeZone, Utc};

/// Сдвиг "локального времени" относительно UTC, в секундах.
pub const LOCAL_OFFSET_SECS: i32 = 5 * 3600;

pub fn local_offset() -> FixedOffset {
    FixedOffset::east_opt(LOCAL_OFFSET_SECS).expect("valid offset")
}

/// Сегодняшняя дата по локальному времени.
pub fn today_local() -> NaiveDate {
    Utc::now().with_timezone(&local_offset()).date_naive()
}

/// Превратить локальные дату + время в UTC-инстант. Например, 09:00 локального
/// 2026-06-15 → DateTime<Utc> момента "09:00 +5".
pub fn local_at(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    let t = NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
    let local = local_offset().from_local_datetime(&date.and_time(t)).unwrap();
    local.with_timezone(&Utc)
}

/// Отформатировать UTC-инстант как локальную дату YYYY-MM-DD.
pub fn fmt_date_local(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&local_offset()).format("%Y-%m-%d").to_string()
}
