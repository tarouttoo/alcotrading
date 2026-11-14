//! UTC-день записи: файл Parquet именуется датой дня, retention считает
//! возраст по той же дате. Календарная арифметика — григорианский календарь
//! по алгоритму Хиннанта, без внешних зависимостей.

/// Микросекунд в UTC-сутках.
pub const US_PER_DAY: i64 = 86_400_000_000;

/// Номер UTC-дня (дней от эпохи 1970-01-01) для момента в микросекундах.
pub fn day_of(us: i64) -> i64 {
    us.div_euclid(US_PER_DAY)
}

/// Календарная дата UTC-дня: `YYYY-MM-DD`.
pub fn format_day(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Дата `YYYY-MM-DD` → номер UTC-дня; `None` для всего, что не дата
/// в форме 4-2-2 и не существует в календаре.
pub fn parse_day(s: &str) -> Option<i64> {
    let mut parts = s.split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some()
        || y.len() != 4
        || m.len() != 2
        || d.len() != 2
        || !(1..=12).contains(&m.parse::<u32>().ok()?)
        || !(1..=31).contains(&d.parse::<u32>().ok()?)
    {
        return None;
    }
    let (y, m, d) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    let day = days_from_civil(y, m, d);
    // дата должна существовать: 2026-02-31 не календарный день
    (civil_from_days(day) == (y, m, d)).then_some(day)
}

/// Дней от эпохи (Хиннант, 1970-01-01 = 0) для календарной даты.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u64; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64; // [0, 11]
    let doy = (153 * mp + 2) / 5 + u64::from(d) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe as i64 - 719_468
}

/// Календарная дата дня (Хиннант).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (y + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Эпоха и границы суток: момент 0 — 1970-01-01, первая микросекунда
    /// следующих суток — уже следующий день (Q39: никаких float).
    #[test]
    fn day_of_bounds() {
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(US_PER_DAY - 1), 0);
        assert_eq!(day_of(US_PER_DAY), 1);
        assert_eq!(day_of(-1), -1);
    }

    /// Формат и парсинг взаимно обратны, включая високосный день.
    #[test]
    fn format_parse_roundtrip() {
        for day in [-1, 0, 1, 19_723, 20_000] {
            let text = format_day(day);
            assert_eq!(text.len(), 10, "{text}");
            assert_eq!(parse_day(&text), Some(day), "{text}");
        }
        assert_eq!(format_day(19_723), "2024-01-01");
        assert_eq!(parse_day("2024-02-29"), parse_day("2024-02-29"));
        assert_eq!(format_day(parse_day("2024-02-29").unwrap()), "2024-02-29");
    }

    /// Не-даты не парсятся: retention не трогает файлы с чужими именами.
    #[test]
    fn parse_rejects_non_dates() {
        assert_eq!(parse_day("2026-13-01"), None);
        assert_eq!(parse_day("2026-00-10"), None);
        assert_eq!(parse_day("2026-01-32"), None);
        assert_eq!(parse_day("2026-1-1"), None);
        assert_eq!(parse_day("2026-01-01x"), None);
        assert_eq!(parse_day(""), None);
        assert_eq!(parse_day("2026"), None);
    }
}
