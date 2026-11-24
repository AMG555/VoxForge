//! Native Malayalam (`ml`) cardinal, decimal, and percentage number verbalizer.
//!
//! Handles atomic chillu endings (`ൽ`, `ൻ`, `ർ`) and sandhi vowel-bridge fusion
//! (`അൻപത്തി` + `ഒന്ന്` → `അൻപത്തിയൊന്ന്`, `നൂറ്റി` + `അഞ്ച്` → `നൂറ്റിയഞ്ച്`) so Malayalam
//! neural/Edge voices pronounce numerals cleanly rather than spelling digits in English.

use regex::Regex;
use std::sync::LazyLock;

static ML_PCT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)%").expect("valid ml percent regex"));
static ML_DEC_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d+)\.(\d+)\b").expect("valid ml decimal regex"));
static ML_INT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,6})\b").expect("valid ml int regex"));

const UNITS: &[&str] = &[
    "പൂജ്യം",
    "ഒന്ന്",
    "രണ്ട്",
    "മൂന്ന്",
    "നാല്",
    "അഞ്ച്",
    "ആറ്",
    "ഏഴ്",
    "എട്ട്",
    "ഒൻപത്",
];

const TEENS: &[&str] = &[
    "പത്ത്",
    "പതിനൊന്ന്",
    "പന്ത്രണ്ട്",
    "പതിമൂന്ന്",
    "പതിനാല്",
    "പതിനഞ്ച്",
    "പതിനാറ്",
    "പതിനേഴ്",
    "പതിനെട്ട്",
    "പത്തൊൻപത്",
];

fn tens_pair(t: usize) -> Option<(&'static str, &'static str)> {
    match t {
        2 => Some(("ഇരുപത്", "ഇരുപത്തി")),
        3 => Some(("മുപ്പത്", "മുപ്പത്തി")),
        4 => Some(("നാല്പത്", "നാല്പത്തി")),
        5 => Some(("അൻപത്", "അൻപത്തി")),
        6 => Some(("അറുപത്", "അറുപത്തി")),
        7 => Some(("എഴുപത്", "എഴുപത്തി")),
        8 => Some(("എൺപത്", "എൺപത്തി")),
        9 => Some(("തൊണ്ണൂറ്", "തൊണ്ണൂറ്റി")),
        _ => None,
    }
}

fn hundred_prefix(h: usize) -> &'static str {
    match h {
        1 => "നൂറ്റി",
        2 => "ഇരുന്നൂറ്റി",
        3 => "മുന്നൂറ്റി",
        4 => "നാനൂറ്റി",
        5 => "അഞ്ഞൂറ്റി",
        6 => "അറുനൂറ്റി",
        7 => "എഴുനൂറ്റി",
        8 => "എണ്ണൂറ്റി",
        9 => "തൊള്ളായിരത്തി",
        _ => "",
    }
}

fn hundred_standalone(h: usize) -> &'static str {
    match h {
        1 => "നൂറ്",
        2 => "ഇരുന്നൂറ്",
        3 => "മുന്നൂറ്",
        4 => "നാനൂറ്",
        5 => "അഞ്ഞൂറ്",
        6 => "അറുനൂറ്",
        7 => "എഴുനൂറ്",
        8 => "എണ്ണൂറ്",
        9 => "തൊള്ളായിരം",
        _ => "",
    }
}

fn thousands_prefix(k: u32) -> String {
    match k {
        1 => "ആയിരത്തി".to_string(),
        2 => "രണ്ടായിരത്തി".to_string(),
        3 => "മൂവായിരത്തി".to_string(),
        4 => "നാലായിരത്തി".to_string(),
        5 => "അയ്യായിരത്തി".to_string(),
        6 => "ആറായിരത്തി".to_string(),
        7 => "ഏഴായിരത്തി".to_string(),
        8 => "എണ്ണായിരത്തി".to_string(),
        9 => "ഒൻപതിനായിരത്തി".to_string(),
        _ => format!("{} ആയിരത്തി", number_to_malayalam(k)),
    }
}

fn thousands_standalone(k: u32) -> String {
    match k {
        1 => "ആയിരം".to_string(),
        2 => "രണ്ടായിരം".to_string(),
        3 => "മൂവായിരം".to_string(),
        4 => "നാലായിരം".to_string(),
        5 => "അയ്യായിരം".to_string(),
        6 => "ആറായിരം".to_string(),
        7 => "ഏഴായിരം".to_string(),
        8 => "എണ്ണായിരം".to_string(),
        9 => "ഒൻപതിനായിരം".to_string(),
        _ => format!("{} ആയിരം", number_to_malayalam(k)),
    }
}

/// Fuse Malayalam combining stem with a follower using natural sandhi vowel glide (`യ`/`വ`).
fn fuse(prefix: &str, follower: &str) -> String {
    let first = match follower.chars().next() {
        Some(c) => c,
        None => return prefix.to_string(),
    };
    let glide = match first {
        'അ' | 'ആ' | 'ഇ' | 'ഈ' | 'എ' | 'ഏ' | 'ഐ' | 'ഒ' | 'ഓ' | 'ഔ' => "യ",
        'ഉ' | 'ഊ' => "വ",
        _ => return format!("{prefix} {follower}"),
    };
    let p_last = prefix.chars().last();
    if p_last == Some('ി') {
        // Strip leading independent vowel and attach vowel sign onto `യ`/`വ` where clean
        let rest: String = follower.chars().skip(1).collect();
        let vowel_sign = match first {
            'അ' => "",
            'ആ' => "ാ",
            'ഇ' => "ി",
            'ഈ' => "ീ",
            'എ' => "െ",
            'ഏ' => "േ",
            'ഐ' => "ൈ",
            'ഒ' => "ൊ",
            'ഓ' => "ോ",
            'ഔ' => "ൗ",
            'ഉ' => "ു",
            'ഊ' => "ൂ",
            _ => "",
        };
        return format!("{prefix}{glide}{vowel_sign}{rest}");
    }
    format!("{prefix} {follower}")
}

/// Convert cardinal integer (0..=999_999) into natural Malayalam words.
pub fn number_to_malayalam(n: u32) -> String {
    if n < 10 {
        return UNITS[n as usize].to_string();
    }
    if n < 20 {
        return TEENS[(n - 10) as usize].to_string();
    }
    if n < 100 {
        let t = (n / 10) as usize;
        let u = (n % 10) as usize;
        if let Some((standalone, comb)) = tens_pair(t) {
            if u == 0 {
                return standalone.to_string();
            }
            return fuse(comb, UNITS[u]);
        }
    }
    if n < 1_000 {
        let h = (n / 100) as usize;
        let rem = n % 100;
        if rem == 0 {
            return hundred_standalone(h).to_string();
        }
        return fuse(hundred_prefix(h), &number_to_malayalam(rem));
    }
    if n < 100_000 {
        let k = n / 1_000;
        let rem = n % 1_000;
        if rem == 0 {
            return thousands_standalone(k);
        }
        if rem < 100 {
            return fuse(&thousands_prefix(k), &number_to_malayalam(rem));
        }
        return format!("{} {}", thousands_prefix(k), number_to_malayalam(rem));
    }
    if n < 1_000_000 {
        let lakhs = n / 100_000;
        let rem = n % 100_000;
        let prefix = if lakhs == 1 {
            if rem == 0 {
                "ഒരു ലക്ഷം".to_string()
            } else {
                "ഒരു ലക്ഷത്തി".to_string()
            }
        } else if rem == 0 {
            format!("{} ലക്ഷം", number_to_malayalam(lakhs))
        } else {
            format!("{} ലക്ഷത്തി", number_to_malayalam(lakhs))
        };
        if rem == 0 {
            return prefix;
        }
        return format!("{prefix} {}", number_to_malayalam(rem));
    }
    n.to_string()
}

/// Convert a decimal token (`"3.14"`) into Malayalam words (`"മൂന്ന് ദശാംശം ഒന്ന് നാല്"`).
pub fn decimal_to_malayalam(int_part: u32, frac_str: &str) -> String {
    let head = number_to_malayalam(int_part);
    let digits: Vec<&str> = frac_str
        .chars()
        .filter_map(|c| c.to_digit(10).map(|d| UNITS[d as usize]))
        .collect();
    if digits.is_empty() {
        head
    } else {
        format!("{head} ദശാംശം {}", digits.join(" "))
    }
}

/// Replace percentages, decimals, and cardinal numbers inside Malayalam prose with words.
pub fn replace_numbers_malayalam(text: &str) -> String {
    let s = ML_PCT_RE
        .replace_all(text, |caps: &regex::Captures<'_>| {
            let num_str = &caps[1];
            if let Some((a, b)) = num_str.split_once('.') {
                let int_val = a.parse::<u32>().unwrap_or(0);
                format!("{} ശതമാനം", decimal_to_malayalam(int_val, b))
            } else if let Ok(v) = num_str.parse::<u32>() {
                format!("{} ശതമാനം", number_to_malayalam(v))
            } else {
                caps[0].to_string()
            }
        })
        .to_string();

    let s = ML_DEC_RE
        .replace_all(&s, |caps: &regex::Captures<'_>| {
            let int_val = caps[1].parse::<u32>().unwrap_or(0);
            decimal_to_malayalam(int_val, &caps[2])
        })
        .to_string();

    ML_INT_RE
        .replace_all(&s, |caps: &regex::Captures<'_>| {
            if let Ok(v) = caps[1].parse::<u32>() {
                number_to_malayalam(v)
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
    fn malayalam_cardinals_and_sandhi() {
        assert_eq!(number_to_malayalam(0), "പൂജ്യം");
        assert_eq!(number_to_malayalam(11), "പതിനൊന്ന്");
        assert_eq!(number_to_malayalam(42), "നാല്പത്തി രണ്ട്");
        assert_eq!(number_to_malayalam(21), "ഇരുപത്തിയൊന്ന്");
        assert_eq!(number_to_malayalam(100), "നൂറ്");
        assert_eq!(number_to_malayalam(105), "നൂറ്റിയഞ്ച്");
        assert_eq!(number_to_malayalam(1000), "ആയിരം");
        assert_eq!(number_to_malayalam(2026), "രണ്ടായിരത്തിയിരുപത്തിയാറ്");
        assert_eq!(number_to_malayalam(100_000), "ഒരു ലക്ഷം");
    }

    #[test]
    fn malayalam_decimals_and_percents() {
        let out = replace_numbers_malayalam("ഇന്ന് 3.14 അല്ലെങ്കിൽ 50% കിഴിവ് 2026 വരെ.");
        assert!(out.contains("മൂന്ന് ദശാംശം ഒന്ന് നാല്"), "got: {out}");
        assert!(out.contains("അൻപത് ശതമാനം"), "got: {out}");
        assert!(out.contains("രണ്ടായിരത്തിയിരുപത്തിയാറ്"), "got: {out}");
    }
}
