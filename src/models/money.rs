use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Uzs,
    Rub,
    Usd,
    Eur,
}

impl Currency {
    /// Сколько минимальных единиц в одной "крупной" (1 USD = 100 центов).
    pub fn minor_per_major(self) -> i64 {
        match self {
            Currency::Uzs => 100, // тийины
            Currency::Rub => 100, // копейки
            Currency::Usd => 100, // центы
            Currency::Eur => 100, // центы
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Currency::Uzs => "сум",
            Currency::Rub => "₽",
            Currency::Usd => "$",
            Currency::Eur => "€",
        }
    }

    pub fn parse(s: &str) -> Option<Currency> {
        match s.to_uppercase().as_str() {
            "UZS" | "СУМ" => Some(Currency::Uzs),
            "RUB" | "RUR" | "₽" => Some(Currency::Rub),
            "USD" | "$" => Some(Currency::Usd),
            "EUR" | "€" => Some(Currency::Eur),
            _ => None,
        }
    }
}

/// Сумма денег в минимальных единицах (тийины/копейки/центы).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount_minor: i64,
    pub currency: Currency,
}

impl Money {
    pub fn new(amount_minor: i64, currency: Currency) -> Self {
        Self { amount_minor, currency }
    }

    /// Формат для отображения: "12 345,67 ₽" / "150 000 сум".
    pub fn format(&self) -> String {
        let per = self.currency.minor_per_major();
        let major = self.amount_minor / per;
        let minor = (self.amount_minor % per).abs();
        let major_str = group_thousands(major);
        if minor == 0 {
            format!("{} {}", major_str, self.currency.symbol())
        } else {
            format!("{},{:02} {}", major_str, minor, self.currency.symbol())
        }
    }

    /// Распарсить ввод пользователя: "12345", "12 345.67", "12345,5".
    pub fn parse(s: &str, currency: Currency) -> Option<Money> {
        let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        let cleaned = cleaned.replace(',', ".");
        let per = currency.minor_per_major();
        if let Some((whole, frac)) = cleaned.split_once('.') {
            let whole_v: i64 = whole.parse().ok()?;
            // Дополняем/обрезаем дробную часть до длины minor.
            let minor_len = (per as f64).log10().round() as usize;
            let mut frac_buf = frac.to_string();
            while frac_buf.len() < minor_len {
                frac_buf.push('0');
            }
            frac_buf.truncate(minor_len);
            let frac_v: i64 = if frac_buf.is_empty() { 0 } else { frac_buf.parse().ok()? };
            let sign = if whole_v < 0 || whole.starts_with('-') { -1 } else { 1 };
            let amount = whole_v.unsigned_abs() as i64 * per + frac_v;
            Some(Money::new(sign * amount, currency))
        } else {
            let whole_v: i64 = cleaned.parse().ok()?;
            Some(Money::new(whole_v * per, currency))
        }
    }
}

fn group_thousands(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let s = n.unsigned_abs().to_string();
    let chars: Vec<char> = s.chars().rev().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(' ');
        }
        out.push(*c);
    }
    format!("{sign}{}", out.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_integer() {
        let m = Money::parse("12345", Currency::Uzs).unwrap();
        assert_eq!(m.amount_minor, 1_234_500);
    }

    #[test]
    fn parses_fractional() {
        let m = Money::parse("12,5", Currency::Rub).unwrap();
        assert_eq!(m.amount_minor, 1250);
        let m = Money::parse("12.05", Currency::Rub).unwrap();
        assert_eq!(m.amount_minor, 1205);
    }

    #[test]
    fn formats_grouped() {
        let m = Money::new(1_234_500, Currency::Uzs);
        assert_eq!(m.format(), "12 345 сум");
        let m = Money::new(1205, Currency::Rub);
        assert_eq!(m.format(), "12,05 ₽");
    }
}
