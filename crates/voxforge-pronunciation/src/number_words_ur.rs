//! Native Urdu (`ur`) cardinal, decimal, and percentage number verbalizer.
//!
//! Translates ASCII and Eastern Arabic / Persian numerals (`۰..۹`, `٠..٩`) up to 999,999
//! into authentic Urdu words (`صفر` .. `نو لاکھ ننانوے ہزار نو سو ننانوے`) because generic
//! multilingual TTS backends frequently struggle with Urdu compound teens and twenties (`پچیس`, `اڑتیس`).

use regex::Regex;
use std::sync::LazyLock;

static UR_PCT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)[%\u{066A}]").expect("valid ur percent regex"));
static UR_DEC_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d+)\.(\d+)\b").expect("valid ur decimal regex"));
static UR_INT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,6})\b").expect("valid ur int regex"));

const UNDER_100: &[&str] = &[
    "صفر",
    "ایک",
    "دو",
    "تین",
    "چار",
    "پانچ",
    "چھ",
    "سات",
    "آٹھ",
    "نو",
    "دس",
    "گیارہ",
    "بارہ",
    "تیرہ",
    "چودہ",
    "پندرہ",
    "سولہ",
    "سترہ",
    "اٹھارہ",
    "انیس",
    "بیس",
    "اکیس",
    "بائیس",
    "تئیس",
    "چوبیس",
    "پچیس",
    "چھبیس",
    "ستائیس",
    "اٹھائیس",
    "انتیس",
    "تیس",
    "اکتیس",
    "بتیس",
    "تینتیس",
    "چونتیس",
    "پینتیس",
    "چھتیس",
    "سینتیس",
    "اڑتیس",
    "انتالیس",
    "چالیس",
    "اکتالیس",
    "بیالیس",
    "تینتالیس",
    "چوالیس",
    "پینتالیس",
    "چھیالیس",
    "سینتالیس",
    "اڑتالیس",
    "انچاس",
    "پچاس",
    "اکیاون",
    "باون",
    "ترپن",
    "چون",
    "پچپن",
    "چھپن",
    "ستاون",
    "اٹھاون",
    "انسٹھ",
    "ساٹھ",
    "اکسٹھ",
    "باسٹھ",
    "ترسٹھ",
    "چونسٹھ",
    "پینسٹھ",
    "چھیاسٹھ",
    "سڑسٹھ",
    "اڑسٹھ",
    "انہتر",
    "ستر",
    "اکہتر",
    "بہتر",
    "تہتر",
    "چوہتر",
    "پچہتر",
    "چھہتر",
    "ستتر",
    "اٹھہتر",
    "اناسی",
    "اسی",
    "اکیاسی",
    "بیاسی",
    "تراسی",
    "چوراسی",
    "پچاسی",
    "چھیاسی",
    "ستاسی",
    "اٹھاسی",
    "نواسی",
    "نوے",
    "اکیانوے",
    "بانوے",
    "ترانوے",
    "چورانوے",
    "پچانوے",
    "چھیانوے",
    "ستانوے",
    "اٹھانوے",
    "ننانوے",
];

/// Normalize Eastern Arabic and Extended Arabic-Indic numerals (`٠..٩`, `۰..۹`) to ASCII digits.
pub fn normalize_arabic_digits(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '٠'..='٩' => char::from_digit((c as u32) - ('٠' as u32), 10).unwrap_or(c),
            '۰'..='۹' => char::from_digit((c as u32) - ('۰' as u32), 10).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// Convert integer (0..=999_999) into natural Urdu words.
pub fn number_to_urdu(n: u32) -> String {
    if n < 100 {
        return UNDER_100[n as usize].to_string();
    }
    if n < 1_000 {
        let h = n / 100;
        let rem = n % 100;
        if rem == 0 {
            return format!("{} سو", UNDER_100[h as usize]);
        }
        return format!("{} سو {}", UNDER_100[h as usize], number_to_urdu(rem));
    }
    if n < 100_000 {
        let k = n / 1_000;
        let rem = n % 1_000;
        if rem == 0 {
            return format!("{} ہزار", number_to_urdu(k));
        }
        return format!("{} ہزار {}", number_to_urdu(k), number_to_urdu(rem));
    }
    if n < 1_000_000 {
        let lakhs = n / 100_000;
        let rem = n % 100_000;
        if rem == 0 {
            return format!("{} لاکھ", number_to_urdu(lakhs));
        }
        return format!("{} لاکھ {}", number_to_urdu(lakhs), number_to_urdu(rem));
    }
    n.to_string()
}

/// Convert decimal (`int_part`, `frac_str`) to Urdu words (`تین اعشاریہ ایک چار`).
pub fn decimal_to_urdu(int_part: u32, frac_str: &str) -> String {
    let head = number_to_urdu(int_part);
    let digits: Vec<&str> = frac_str
        .chars()
        .filter_map(|c| c.to_digit(10).map(|d| UNDER_100[d as usize]))
        .collect();
    if digits.is_empty() {
        head
    } else {
        format!("{head} اعشاریہ {}", digits.join(" "))
    }
}

/// Replace numbers, decimals, percentages, and Urdu/Arabic digits in Urdu text with spoken words.
pub fn replace_numbers_urdu(text: &str) -> String {
    let ascii = normalize_arabic_digits(text);
    let s = UR_PCT_RE
        .replace_all(&ascii, |caps: &regex::Captures<'_>| {
            let num_str = &caps[1];
            if let Some((a, b)) = num_str.split_once('.') {
                let int_val = a.parse::<u32>().unwrap_or(0);
                format!("{} فیصد", decimal_to_urdu(int_val, b))
            } else if let Ok(v) = num_str.parse::<u32>() {
                format!("{} فیصد", number_to_urdu(v))
            } else {
                caps[0].to_string()
            }
        })
        .to_string();

    let s = UR_DEC_RE
        .replace_all(&s, |caps: &regex::Captures<'_>| {
            let int_val = caps[1].parse::<u32>().unwrap_or(0);
            decimal_to_urdu(int_val, &caps[2])
        })
        .to_string();

    UR_INT_RE
        .replace_all(&s, |caps: &regex::Captures<'_>| {
            if let Ok(v) = caps[1].parse::<u32>() {
                number_to_urdu(v)
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urdu_cardinals_and_arabic_numerals() {
        assert_eq!(number_to_urdu(0), "صفر");
        assert_eq!(number_to_urdu(25), "پچیس");
        assert_eq!(number_to_urdu(42), "بیالیس");
        assert_eq!(number_to_urdu(150), "ایک سو پچاس");
        assert_eq!(number_to_urdu(2026), "دو ہزار چھبیس");
        let out = replace_numbers_urdu("آج ۲۵٪ یا 3.14 ہے");
        assert!(out.contains("پچیس فیصد"), "got: {out}");
        assert!(out.contains("تین اعشاریہ ایک چار"), "got: {out}");
    }
}
