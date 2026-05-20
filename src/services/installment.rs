//! Парсер графика рассрочки и построение списка платежей.

use crate::models::Installment;
use crate::services::clock::{local_at, today_local};
use chrono::{DateTime, Duration, NaiveDate, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct InstallmentPlan {
    pub count: u32,
    pub period_days: u32,
    /// Дата первого платежа. Если `None` — first = today + period_days.
    pub first_due: Option<DateTime<Utc>>,
}

/// Парсинг короткого формата:
///   "3/30"             → 3 платежа каждые 30 дней, первый через 30 дней от сегодня
///   "3/30/2026-06-15"  → 3 платежа каждые 30 дней, первый 15 июня 2026
pub fn parse_plan(s: &str) -> Option<InstallmentPlan> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let parts: Vec<&str> = cleaned.split('/').collect();
    if parts.len() < 2 || parts.len() > 3 { return None; }
    let count: u32 = parts[0].parse().ok()?;
    let period: u32 = parts[1].parse().ok()?;
    if count == 0 || count > 60 || period == 0 || period > 365 { return None; }
    let first = if parts.len() == 3 {
        let nd = NaiveDate::parse_from_str(parts[2], "%Y-%m-%d").ok()?;
        // 09:00 локального времени (Asia/Tashkent, UTC+5) → 04:00 UTC.
        Some(local_at(nd, 9, 0))
    } else {
        None
    };
    Some(InstallmentPlan { count, period_days: period, first_due: first })
}

/// Построить вектор `Installment` из плана и суммы долга. Остаток от деления
/// добавляется к последнему платежу — тогда сумма точно равна `total_minor`.
pub fn build_installments(plan: &InstallmentPlan, total_minor: i64) -> Vec<Installment> {
    let n = plan.count as i64;
    if n <= 0 || total_minor <= 0 { return Vec::new(); }

    let base = total_minor / n;
    let remainder = total_minor - base * n;

    let first = plan.first_due.unwrap_or_else(|| {
        // Сегодня (по локальному времени) + period_days, в 09:00 локального.
        let due = today_local() + Duration::days(plan.period_days as i64);
        local_at(due, 9, 0)
    });

    let mut out = Vec::with_capacity(plan.count as usize);
    for i in 0..plan.count {
        let due = first + Duration::days((i as i64) * (plan.period_days as i64));
        let amount = if i == plan.count - 1 { base + remainder } else { base };
        out.push(Installment {
            due_date: due,
            amount_minor: amount,
            paid: false,
            due_notified_at: None,
            overdue_notified_at: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short() {
        let p = parse_plan("3/30").unwrap();
        assert_eq!(p.count, 3);
        assert_eq!(p.period_days, 30);
        assert!(p.first_due.is_none());
    }

    #[test]
    fn parses_with_date() {
        let p = parse_plan("4 / 15 / 2026-07-01").unwrap();
        assert_eq!(p.count, 4);
        assert_eq!(p.period_days, 15);
        assert!(p.first_due.is_some());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_plan("abc").is_none());
        assert!(parse_plan("3").is_none());
        assert!(parse_plan("0/30").is_none());
        assert!(parse_plan("3/0").is_none());
        assert!(parse_plan("3/30/notadate").is_none());
    }

    #[test]
    fn builds_sums_exactly() {
        let plan = InstallmentPlan { count: 3, period_days: 30, first_due: None };
        let xs = build_installments(&plan, 1000);
        let sum: i64 = xs.iter().map(|x| x.amount_minor).sum();
        assert_eq!(sum, 1000);
        // Все, кроме последнего, равны
        assert_eq!(xs[0].amount_minor, xs[1].amount_minor);
    }
}
