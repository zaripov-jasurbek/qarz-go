use crate::models::money::Currency;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum DebtSource {
    Manual,
    FromRoom { room_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebtStatus {
    /// Активный долг.
    Confirmed,
    /// Полностью погашен.
    Settled,
    /// Должник оспорил.
    Disputed,
    /// Прощён кредитором.
    Forgiven,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installment {
    pub due_date: DateTime<Utc>,
    pub amount_minor: i64,
    pub paid: bool,
    /// Когда отправили "сегодня день оплаты". `None` = ещё не слали.
    #[serde(default)]
    pub due_notified_at: Option<DateTime<Utc>>,
    /// Когда отправили "просрочено". `None` = ещё не слали.
    #[serde(default)]
    pub overdue_notified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payment {
    pub amount_minor: i64,
    pub at: DateTime<Utc>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Debt {
    pub id: String,
    pub debtor_user_id: String,
    pub creditor_user_id: String,
    pub original_amount_minor: i64,
    pub currency: Currency,
    pub description: String,
    pub source: DebtSource,
    /// Пустой = одной суммой без графика.
    pub installments: Vec<Installment>,
    pub payments: Vec<Payment>,
    pub status: DebtStatus,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

impl Debt {
    pub fn new(
        debtor: String,
        creditor: String,
        amount_minor: i64,
        currency: Currency,
        description: String,
        source: DebtSource,
    ) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            debtor_user_id: debtor,
            creditor_user_id: creditor,
            original_amount_minor: amount_minor,
            currency,
            description,
            source,
            installments: Vec::new(),
            payments: Vec::new(),
            status: DebtStatus::Confirmed,
            created_at: Utc::now(),
            settled_at: None,
        }
    }

    pub fn total_paid_minor(&self) -> i64 {
        self.payments.iter().map(|p| p.amount_minor).sum()
    }

    pub fn remaining_minor(&self) -> i64 {
        (self.original_amount_minor - self.total_paid_minor()).max(0)
    }

    /// Пересчитать флаг `paid` у каждой рассрочки на основе суммы payments.
    /// Платежи списываются с ближайшего по due_date неоплаченного. Если общая
    /// сумма payments покрывает накопленную сумму рассрочек до позиции `i`
    /// включительно — позиция считается оплаченной.
    pub fn recompute_installment_status(&mut self) {
        if self.installments.is_empty() { return; }
        let total_paid = self.total_paid_minor();
        // Сортировка по due_date только для расчёта; не меняем порядок в хранилище.
        let mut order: Vec<usize> = (0..self.installments.len()).collect();
        order.sort_by_key(|&i| self.installments[i].due_date);
        let mut acc: i64 = 0;
        for idx in order {
            acc += self.installments[idx].amount_minor;
            self.installments[idx].paid = total_paid >= acc;
        }
    }
}
