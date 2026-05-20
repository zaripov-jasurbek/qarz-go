//! Расчёт долей и упрощение цепочек долгов.

use crate::models::{Currency, RoomItem};
use std::collections::HashMap;

/// Доля одного пользователя в одной позиции.
#[derive(Debug, Clone)]
pub struct ItemShare {
    pub user_id: String,
    pub amount_minor: i64,
}

/// Разбить цену позиции между теми, кто её выбрал. Сумма долей точно равна
/// total_price_minor — остаток от деления добавляем к первому в (сортированном)
/// списке `selected_by`, чтобы результат был детерминированным.
pub fn split_item(item: &RoomItem) -> Vec<ItemShare> {
    if item.selected_by.is_empty() || item.total_price_minor <= 0 {
        return Vec::new();
    }
    let mut ids: Vec<String> = item.selected_by.clone();
    ids.sort(); // детерминизм
    let n = ids.len() as i64;
    let base = item.total_price_minor / n;
    let rem = item.total_price_minor - base * n;
    ids.into_iter()
        .enumerate()
        .map(|(i, user_id)| ItemShare {
            user_id,
            amount_minor: base + if (i as i64) < rem { 1 } else { 0 },
        })
        .collect()
}

/// Сводка по комнате: сколько каждый участник должен по всем позициям.
/// Возвращает map user_id → сколько он должен оплатить (в minor).
pub fn room_totals(items: &[RoomItem]) -> HashMap<String, i64> {
    let mut totals: HashMap<String, i64> = HashMap::new();
    for item in items {
        for share in split_item(item) {
            *totals.entry(share.user_id).or_insert(0) += share.amount_minor;
        }
    }
    totals
}

/// Перевод между двумя людьми после упрощения цепочки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    pub from_user_id: String,
    pub to_user_id: String,
    pub amount_minor: i64,
    pub currency: Currency,
}

/// Greedy-упрощение: на входе net-баланс каждого юзера (положительный = должны
/// ему, отрицательный = он должен). На выходе — минимальный (приблизительно)
/// набор переводов, обнуляющий все балансы. Это NP-hard в общем случае, greedy
/// работает быстро и почти всегда оптимально для бытовых сценариев.
pub fn simplify(balances: HashMap<String, i64>, currency: Currency) -> Vec<Transfer> {
    let mut creditors: Vec<(String, i64)> = balances.iter()
        .filter(|(_, &v)| v > 0)
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let mut debtors: Vec<(String, i64)> = balances.iter()
        .filter(|(_, &v)| v < 0)
        .map(|(k, v)| (k.clone(), -*v))
        .collect();

    // Сортируем по убыванию суммы для лучшей сходимости.
    creditors.sort_by(|a, b| b.1.cmp(&a.1));
    debtors.sort_by(|a, b| b.1.cmp(&a.1));

    let mut transfers = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < debtors.len() && j < creditors.len() {
        let pay = debtors[i].1.min(creditors[j].1);
        if pay > 0 {
            transfers.push(Transfer {
                from_user_id: debtors[i].0.clone(),
                to_user_id: creditors[j].0.clone(),
                amount_minor: pay,
                currency,
            });
            debtors[i].1 -= pay;
            creditors[j].1 -= pay;
        }
        if debtors[i].1 == 0 { i += 1; }
        if creditors[j].1 == 0 { j += 1; }
    }
    transfers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(price: i64, selected: &[&str]) -> RoomItem {
        RoomItem {
            id: "x".into(),
            room_id: "r".into(),
            name: "x".into(),
            quantity: None,
            unit: None,
            total_price_minor: price,
            selected_by: selected.iter().map(|s| s.to_string()).collect(),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn splits_evenly() {
        let shares = split_item(&item(900, &["a", "b", "c"]));
        assert_eq!(shares.iter().map(|s| s.amount_minor).sum::<i64>(), 900);
        for s in &shares { assert_eq!(s.amount_minor, 300); }
    }

    #[test]
    fn splits_with_remainder() {
        // 1000 / 3 = 333 + 333 + 334 (или 334 + 333 + 333) — детерминизм важен.
        let shares = split_item(&item(1000, &["b", "a", "c"]));
        assert_eq!(shares.iter().map(|s| s.amount_minor).sum::<i64>(), 1000);
        // Первый отсортированный = "a", получает дополнительные единицы.
        assert_eq!(shares[0].user_id, "a");
    }

    #[test]
    fn empty_selection_no_shares() {
        let shares = split_item(&item(1000, &[]));
        assert!(shares.is_empty());
    }

    #[test]
    fn simplifies_chain() {
        // A должен 100, B должен 0, C получит 100.
        let mut bal = HashMap::new();
        bal.insert("A".to_string(), -100);
        bal.insert("B".to_string(), 0);
        bal.insert("C".to_string(), 100);
        let xs = simplify(bal, Currency::Uzs);
        assert_eq!(xs.len(), 1);
        assert_eq!(xs[0].from_user_id, "A");
        assert_eq!(xs[0].to_user_id, "C");
        assert_eq!(xs[0].amount_minor, 100);
    }
}
