use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    /// Владелец адресной книги.
    pub owner_user_id: String,
    /// Как владелец назвал этот контакт.
    pub display_name: String,
    /// Из Telegram shared contact.
    pub phone: String,
    /// Если контакт сам зарегался в боте — линкуем сюда его User.id.
    pub linked_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Contact {
    pub fn new(owner_user_id: String, display_name: String, phone: String) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            owner_user_id,
            display_name,
            phone: normalize_phone(&phone),
            linked_user_id: None,
            created_at: Utc::now(),
        }
    }
}

/// Нормализует номер телефона в формат `+XXXXXXXXXXXX`.
///
/// Поддерживаемые форматы:
/// - `+998 90 336 36 39`  → `+998903363639`
/// - `998-90-336-36-39`   → `+998903363639`
/// - `0903363639`          → `+998903363639`  (UZ локальный с 0)
/// - `903363639`           → `+998903363639`  (UZ локальный без 0)
/// - `89031234567`         → `+79031234567`   (RU формат с 8)
pub fn normalize_phone(s: &str) -> String {
    // Оставляем только цифры — пробелы, дефисы, скобки, точки отбрасываем.
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();

    match digits.len() {
        0 => String::new(),
        // 9 цифр — UZ без кода страны и без ведущего 0: 903363639
        9 => format!("+998{}", digits),
        // 10 цифр с ведущим 0 — UZ локальный: 0903363639
        10 if digits.starts_with('0') => format!("+998{}", &digits[1..]),
        // 11 цифр с ведущей 8 — RU/KZ формат: 89031234567
        11 if digits.starts_with('8') => format!("+7{}", &digits[1..]),
        // Всё остальное (12+ цифр, уже с кодом страны): просто добавляем +
        _ => format!("+{}", digits),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_phone;

    #[test]
    fn test_normalize_phone() {
        // Международный с пробелами
        assert_eq!(normalize_phone("+998 90 336 36 39"), "+998903363639");
        // Международный с дефисами
        assert_eq!(normalize_phone("+998-90-336-36-39"), "+998903363639");
        // Международный без пробелов
        assert_eq!(normalize_phone("+998903363639"), "+998903363639");
        // Без плюса, с кодом страны
        assert_eq!(normalize_phone("998903363639"), "+998903363639");
        // UZ локальный с 0
        assert_eq!(normalize_phone("0903363639"), "+998903363639");
        // UZ локальный без 0 и без кода
        assert_eq!(normalize_phone("903363639"), "+998903363639");
        // RU формат с 8
        assert_eq!(normalize_phone("89031234567"), "+79031234567");
        // Пустая строка
        assert_eq!(normalize_phone(""), "");
    }
}
