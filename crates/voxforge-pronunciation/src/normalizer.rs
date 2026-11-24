//! Text normalization: symbols, currency, numbers, abbreviations.
//!
//! All transforms are deterministic and pure — no I/O, no state.

use regex::Regex;
use std::sync::{LazyLock, OnceLock};

/// Currency symbols mapped to their spoken form.
static CURRENCY_MAP: &[(&str, &str)] = &[
    ("₹", "rupees"),
    ("$", "dollars"),
    ("€", "euros"),
    ("£", "pounds"),
    ("¥", "yen"),
    ("₩", "won"),
    ("₺", "lira"),
    ("฿", "baht"),
];

/// Compiled regex patterns (lazily initialised).
fn re_number_k() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?)[Kk]\b").unwrap())
}

fn re_number_m() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?)[Mm]\b").unwrap())
}

fn re_number_b() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?)[Bb]\b").unwrap())
}

fn re_currency_amount() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Matches currency symbols immediately followed by a number (e.g. $1,000 or ₹50.00)
    RE.get_or_init(|| Regex::new(r"([₹$€£¥₩₺฿])([\d,]+(?:\.\d{1,2})?)").unwrap())
}

fn re_comma_number() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d),(\d{3})").unwrap())
}

fn re_percent() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?)%").unwrap())
}

fn re_url() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"https?://(?:www\.)?([^\s]+)").unwrap())
}

fn re_email() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b([a-zA-Z0-9._%+-]+)@([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})\b").unwrap()
    })
}

fn re_clock_time() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2}):(\d{2})(?:\s*(AM|PM|am|pm))?\b").unwrap())
}

fn re_temporal_year() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(in|since|from|until|by|before|after|around|year)\s+(19\d{2}|20\d{2})\b")
            .unwrap()
    })
}

fn num_to_words(n: u32) -> String {
    match n {
        0 => "zero".to_string(),
        1 => "one".to_string(),
        2 => "two".to_string(),
        3 => "three".to_string(),
        4 => "four".to_string(),
        5 => "five".to_string(),
        6 => "six".to_string(),
        7 => "seven".to_string(),
        8 => "eight".to_string(),
        9 => "nine".to_string(),
        10 => "ten".to_string(),
        11 => "eleven".to_string(),
        12 => "twelve".to_string(),
        13 => "thirteen".to_string(),
        14 => "fourteen".to_string(),
        15 => "fifteen".to_string(),
        16 => "sixteen".to_string(),
        17 => "seventeen".to_string(),
        18 => "eighteen".to_string(),
        19 => "nineteen".to_string(),
        20..=99 => {
            let tens = match n / 10 {
                2 => "twenty",
                3 => "thirty",
                4 => "forty",
                5 => "fifty",
                6 => "sixty",
                7 => "seventy",
                8 => "eighty",
                9 => "ninety",
                _ => "",
            };
            let rem = n % 10;
            if rem == 0 {
                tens.to_string()
            } else {
                format!("{tens}-{}", num_to_words(rem))
            }
        }
        _ => n.to_string(),
    }
}

fn year_to_words(year: u32) -> String {
    if (1900..=1999).contains(&year) {
        let first = "nineteen";
        let last = year % 100;
        if last == 0 {
            format!("{first} hundred")
        } else if last < 10 {
            format!("{first} oh {}", num_to_words(last))
        } else {
            format!("{first} {}", num_to_words(last))
        }
    } else if (2000..=2009).contains(&year) {
        if year == 2000 {
            "two thousand".to_string()
        } else {
            format!("twenty oh {}", num_to_words(year % 100))
        }
    } else if (2010..=2099).contains(&year) {
        format!("twenty {}", num_to_words(year % 100))
    } else {
        year.to_string()
    }
}

/// Stateless text normalizer.
pub struct TextNormalizer;

impl TextNormalizer {
    /// Normalize `text` through all normalization passes using default (English/generic) rules.
    pub fn normalize(text: &str) -> String {
        Self::normalize_for_language(text, None)
    }

    /// Language-aware pre-synthesis text normalization.
    ///
    /// Pass order matters:
    /// 1. Strip unsafe control characters (preserving ZWNJ/ZWJ), HTML entities, markdown emphasis,
    ///    and excess punctuation floods while leaving `[pause ...]` / `[[...]]` bracket cues intact.
    /// 2. Normalize spans outside brackets: URLs, emails, clock times, temporal years (English),
    ///    ranges (`20~30`), abbreviations, currencies, European decimals, and native numbers
    ///    for Malayalam (`ml`) / Urdu (`ur`).
    /// 3. Apply conversational cadence smoothing.
    pub fn normalize_for_language(text: &str, language: Option<&str>) -> String {
        let base_lang = language
            .unwrap_or("en")
            .trim()
            .split(['-', '_'])
            .next()
            .unwrap_or("en")
            .to_ascii_lowercase();

        let out = Self::normalize_outside_brackets(text, |segment| {
            let mut s = Self::sanitize_for_speech(segment);
            s = Self::normalize_urls(&s);
            s = Self::normalize_emails(&s);

            match base_lang.as_str() {
                "ml" | "mal" => {
                    s = Self::normalize_tilde_ranges(&s, "മുതൽ", "");
                    s = Self::normalize_scale_suffixes(&s);
                    s = Self::strip_comma_separators(&s);
                    s = crate::number_words_ml::replace_numbers_malayalam(&s);
                }
                "ur" | "urd" => {
                    s = Self::normalize_tilde_ranges(&s, "سے", "تک");
                    s = Self::normalize_scale_suffixes(&s);
                    s = Self::strip_comma_separators(&s);
                    s = crate::number_words_ur::replace_numbers_urdu(&s);
                }
                "de" | "deu" | "es" | "spa" | "fr" | "fra" | "pt" | "por" | "it" | "ita"
                | "nl" | "nld" | "pl" | "pol" | "ru" | "rus" => {
                    s = Self::normalize_tilde_ranges(&s, match base_lang.as_str() {
                        "de" | "deu" => "bis",
                        "fr" | "fra" => "à",
                        "it" | "ita" => "a",
                        _ => "a",
                    }, "");
                    s = Self::expand_language_abbreviations(&s, &base_lang);
                    s = Self::normalize_european_decimals(&s);
                    s = Self::normalize_currency_amounts(&s);
                    s = Self::normalize_scale_suffixes(&s);
                    s = Self::normalize_percentages(&s);
                    s = Self::strip_comma_separators(&s);
                }
                _ => {
                    s = Self::normalize_tilde_ranges(&s, "to", "");
                    s = Self::expand_language_abbreviations(&s, &base_lang);
                    s = Self::normalize_clock_times(&s);
                    s = Self::normalize_temporal_years(&s);
                    s = Self::normalize_currency_amounts(&s);
                    s = Self::normalize_scale_suffixes(&s);
                    s = Self::normalize_percentages(&s);
                    s = Self::strip_comma_separators(&s);
                }
            }
            s
        });

        Self::humanize_cadence(&out)
    }

    /// Run a transformation only outside `[...]` and `[[...]]` cue/phonetic brackets.
    pub fn normalize_outside_brackets<F>(text: &str, mut f: F) -> String
    where
        F: FnMut(&str) -> String,
    {
        if !text.contains('[') {
            return f(text);
        }
        let mut result = String::with_capacity(text.len());
        let mut cursor = 0usize;
        let bytes = text.as_bytes();
        while cursor < bytes.len() {
            if let Some(rel_open) = text[cursor..].find('[') {
                let open_idx = cursor + rel_open;
                if open_idx > cursor {
                    result.push_str(&f(&text[cursor..open_idx]));
                }
                let is_double = text[open_idx..].starts_with("[[");
                let close_pat = if is_double { "]]" } else { "]" };
                if let Some(rel_close) = text[open_idx + close_pat.len()..].find(close_pat) {
                    let close_end = open_idx + close_pat.len() + rel_close + close_pat.len();
                    result.push_str(&text[open_idx..close_end]);
                    cursor = close_end;
                } else {
                    result.push_str(&f(&text[open_idx..]));
                    break;
                }
            } else {
                result.push_str(&f(&text[cursor..]));
                break;
            }
        }
        result
    }

    /// Strip unsafe control chars (keeping ZWNJ/ZWJ `\u{200C}`/`\u{200D}`), HTML entities,
    /// markdown emphasis, and repeated punctuation floods (`!!!!` -> `!!!`).
    pub fn sanitize_for_speech(text: &str) -> String {
        let mut cleaned = String::with_capacity(text.len());
        for c in text.chars() {
            let cp = c as u32;
            if cp == 0x200C || cp == 0x200D || cp == 0x0A || cp == 0x09 || !c.is_control() {
                cleaned.push(c);
            }
        }

        let decoded = cleaned
            .replace("&amp;", " and ")
            .replace("&nbsp;", " ")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
            .replace("&lt;", " ")
            .replace("&gt;", " ");

        // Strip markdown emphasis without damaging snake_case identifiers
        static RE_MD_BOLD: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"\*\*([^*]+)\*\*|__([^_]+)__").expect("valid bold regex")
        });
        static RE_MD_ITALIC: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?m)(^|\s)\*([^*\n]+)\*($|\s|[.,!?;:])|(^|\s)_([^_\n]+)_($|\s|[.,!?;:])")
                .expect("valid italic regex")
        });
        static RE_EXCL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!{4,}").unwrap());
        static RE_QUEST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\?{4,}").unwrap());
        static RE_DOTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.{4,}").unwrap());

        let s = RE_MD_BOLD
            .replace_all(&decoded, |c: &regex::Captures<'_>| {
                c.get(1).or_else(|| c.get(2)).map(|m| m.as_str()).unwrap_or("").to_string()
            })
            .to_string();
        let s = RE_MD_ITALIC
            .replace_all(&s, |c: &regex::Captures<'_>| {
                if let Some(inner) = c.get(2) {
                    format!("{}{}{}", &c[1], inner.as_str(), &c[3])
                } else {
                    format!(
                        "{}{}{}",
                        c.get(4).map(|m| m.as_str()).unwrap_or(""),
                        c.get(5).map(|m| m.as_str()).unwrap_or(""),
                        c.get(6).map(|m| m.as_str()).unwrap_or("")
                    )
                }
            })
            .to_string();
        let s = RE_EXCL.replace_all(&s, "!!!");
        let s = RE_QUEST.replace_all(&s, "???");
        RE_DOTS.replace_all(&s, "...").to_string()
    }

    fn normalize_tilde_ranges(text: &str, connector: &str, tail: &str) -> String {
        static RE_TILDE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(\d+(?:[.,]\d+)?)\s*[~〜]\s*(\d+(?:[.,]\d+)?)").expect("valid tilde regex")
        });
        RE_TILDE
            .replace_all(text, |c: &regex::Captures<'_>| {
                if tail.is_empty() {
                    format!("{} {connector} {}", &c[1], &c[2])
                } else {
                    format!("{} {connector} {} {tail}", &c[1], &c[2])
                }
            })
            .to_string()
    }

    fn normalize_european_decimals(text: &str) -> String {
        static RE_EURO_DEC: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b(\d+),(\d{1,2})\b").expect("valid euro decimal"));
        RE_EURO_DEC.replace_all(text, "$1.$2").to_string()
    }

    fn expand_language_abbreviations(text: &str, base_lang: &str) -> String {
        static DE_ABBREVS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
            [
                (r"\bz\.B\.", "zum Beispiel"),
                (r"\bbzw\.", "beziehungsweise"),
                (r"\busw\.", "und so weiter"),
                (r"\bca\.", "circa"),
                (r"\bDr\.", "Doktor"),
            ]
            .into_iter()
            .filter_map(|(p, r)| Regex::new(p).ok().map(|re| (re, r)))
            .collect()
        });
        static ES_ABBREVS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
            [
                (r"\bSr\.", "Señor"),
                (r"\bSra\.", "Señora"),
                (r"\bSrta\.", "Señorita"),
                (r"\bDr\.", "Doctor"),
                (r"\bDra\.", "Doctora"),
                (r"\bUd\.", "Usted"),
            ]
            .into_iter()
            .filter_map(|(p, r)| Regex::new(p).ok().map(|re| (re, r)))
            .collect()
        });
        static FR_ABBREVS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
            [
                (r"\bM\.", "Monsieur"),
                (r"\bMme\b\.?", "Madame"),
                (r"\bMlle\b\.?", "Mademoiselle"),
                (r"\bDr\.", "Docteur"),
            ]
            .into_iter()
            .filter_map(|(p, r)| Regex::new(p).ok().map(|re| (re, r)))
            .collect()
        });
        static EN_ABBREVS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
            [
                (r"\bMr\.", "Mister"),
                (r"\bMrs\.", "Missus"),
                (r"\bMs\.", "Miss"),
                (r"\bDr\.", "Doctor"),
                (r"\bProf\.", "Professor"),
                (r"\betc\.", "et cetera"),
                (r"\be\.g\.", "for example"),
                (r"\bi\.e\.", "that is"),
                (r"\bvs\.", "versus"),
            ]
            .into_iter()
            .filter_map(|(p, r)| Regex::new(p).ok().map(|re| (re, r)))
            .collect()
        });

        let pairs = match base_lang {
            "de" | "deu" => &*DE_ABBREVS,
            "es" | "spa" => &*ES_ABBREVS,
            "fr" | "fra" => &*FR_ABBREVS,
            _ => &*EN_ABBREVS,
        };

        let mut out = text.to_string();
        for (re, rep) in pairs {
            out = re.replace_all(&out, *rep).to_string();
        }
        out
    }

    /// Collapse ASR / neural hallucination loops (6+ repeated consecutive words or short phrase cycles).
    pub fn collapse_repetitive_artifacts(text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() < 4 {
            return text.trim().to_string();
        }
        let mut deduped: Vec<&str> = Vec::with_capacity(words.len());
        let mut i = 0usize;
        while i < words.len() {
            let w = words[i];
            deduped.push(w);
            let mut run = 1usize;
            while i + run < words.len() && words[i + run].eq_ignore_ascii_case(w) {
                run += 1;
            }
            if run >= 6 {
                i += run;
            } else {
                i += 1;
            }
        }
        deduped.join(" ")
    }

    /// Humanize text cadence: insert natural breathing pauses for clauses >10 words,
    /// format question cadence, and smooth robotic run-on sentences.
    pub fn humanize_cadence(text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        // Punctuation and breathing cadence for conversational realism
        let mut result = String::new();
        let words: Vec<&str> = trimmed.split_whitespace().collect();

        for (i, word) in words.iter().enumerate() {
            let lower = word.to_lowercase();
            let clean_lower = lower.trim_matches(|c: char| !c.is_alphabetic());

            // Break up overly long clauses at natural conjunctions with micro-pause commas
            // Includes English, Manglish, Hinglish, Tanglish, and native Indic conjunctions
            let is_conjunction = matches!(
                clean_lower,
                // English
                "and" | "but" | "because" | "although" | "however" | "meanwhile" | "whereas" | "since" |
                // Manglish (Malayalam Romanized)
                "ennitt" | "enkilum" | "athukond" | "pinne" | "athinal" | "koodathe" | "allathe" |
                // Hinglish (Hindi Romanized)
                "aur" | "lekin" | "kyunki" | "magar" | "par" | "isliye" | "balki" |
                // Tanglish (Tamil Romanized)
                "aana" | "analum" | "athunala" | "pinna"
            ) || matches!(
                word.trim_matches(|c: char| c == ',' || c == '.' || c == '?' || c == '!'),
                // Native Malayalam script
                "എന്നിട്ട്" | "എങ്കിലും" | "അതുകൊണ്ട്" | "പിന്നെ" | "കൂടാതെ" | "പക്ഷേ" |
                // Native Hindi Devanagari script
                "और" | "लेकिन" | "क्योंकि" | "मगर" | "पर" | "इसलिए"
            );

            if is_conjunction && i > 4 && i < words.len() - 3 {
                let trimmed_res = result.trim_end();
                if !trimmed_res.ends_with(',')
                    && !trimmed_res.ends_with('.')
                    && !trimmed_res.ends_with('?')
                    && !trimmed_res.ends_with('!')
                    && !trimmed_res.ends_with(';')
                    && !trimmed_res.ends_with(':')
                {
                    result = trimmed_res.to_string();
                    result.push(',');
                    result.push(' ');
                }
            }

            result.push_str(word);

            if i < words.len() - 1 {
                result.push(' ');
            }
        }
        result
    }

    /// Convert `$1,000` → `1000 dollars`, `₹50.00` → `50 rupees`, etc.
    fn normalize_currency_amounts(text: &str) -> String {
        re_currency_amount()
            .replace_all(text, |caps: &regex::Captures<'_>| {
                let symbol = caps.get(1).map_or("", |m| m.as_str());
                let amount = caps.get(2).map_or("", |m| m.as_str());
                let spoken_currency = CURRENCY_MAP
                    .iter()
                    .find(|(s, _)| *s == symbol)
                    .map(|(_, name)| *name)
                    .unwrap_or("units");
                // Remove comma separators from amount
                let clean_amount = amount.replace(',', "");
                format!("{clean_amount} {spoken_currency}")
            })
            .to_string()
    }

    /// Convert `1K` → `1000`, `2.5M` → `2500000`, `1B` → `1000000000`.
    fn normalize_scale_suffixes(text: &str) -> String {
        let s = re_number_k().replace_all(text, |caps: &regex::Captures<'_>| {
            let n: f64 = caps[1].parse().unwrap_or(0.0);
            format!("{}", (n * 1_000.0) as u64)
        });
        let s = re_number_m().replace_all(&s, |caps: &regex::Captures<'_>| {
            let n: f64 = caps[1].parse().unwrap_or(0.0);
            format!("{}", (n * 1_000_000.0) as u64)
        });
        re_number_b()
            .replace_all(&s, |caps: &regex::Captures<'_>| {
                let n: f64 = caps[1].parse().unwrap_or(0.0);
                format!("{}", (n * 1_000_000_000.0) as u64)
            })
            .to_string()
    }

    /// Convert `95%` → `95 percent`.
    fn normalize_percentages(text: &str) -> String {
        re_percent().replace_all(text, "$1 percent").to_string()
    }

    /// Strip comma-separated thousands: `1,000` → `1000`.
    fn strip_comma_separators(text: &str) -> String {
        // Apply repeatedly until stable (handles 1,000,000)
        let mut prev = text.to_string();
        loop {
            let next = re_comma_number().replace_all(&prev, "$1$2").to_string();
            if next == prev {
                break;
            }
            prev = next;
        }
        prev
    }

    /// Convert URLs `https://voxforge.org/api` → `voxforge dot org slash api`.
    pub fn normalize_urls(text: &str) -> String {
        re_url()
            .replace_all(text, |caps: &regex::Captures<'_>| {
                let raw = &caps[1];
                let trimmed = raw.trim_end_matches(['.', ',', ';', '!', '?', ')']);
                let trailing = &raw[trimmed.len()..];
                let clean = trimmed.strip_prefix("www.").unwrap_or(trimmed);
                let spoken = clean
                    .replace('.', " dot ")
                    .replace('/', " slash ")
                    .replace('-', " dash ")
                    .replace('_', " underscore ");
                format!("{spoken}{trailing}")
            })
            .to_string()
    }

    /// Convert `user@domain.com` → `user at domain dot com`.
    pub fn normalize_emails(text: &str) -> String {
        re_email()
            .replace_all(text, |caps: &regex::Captures<'_>| {
                let user = &caps[1];
                let domain = &caps[2];
                let spoken_domain = domain.replace('.', " dot ");
                format!("{user} at {spoken_domain}")
            })
            .to_string()
    }

    /// Convert `3:30 PM` → `three thirty PM`, `10:05 AM` → `ten oh five AM`.
    pub fn normalize_clock_times(text: &str) -> String {
        re_clock_time()
            .replace_all(text, |caps: &regex::Captures<'_>| {
                let h: u32 = caps[1].parse().unwrap_or(0);
                let m: u32 = caps[2].parse().unwrap_or(0);
                let period = caps.get(3).map(|m| m.as_str().to_uppercase());

                if h > 23 || m > 59 {
                    return caps[0].to_string();
                }

                let h_val = if h == 0 {
                    12
                } else if h > 12 && period.is_some() {
                    h - 12
                } else {
                    h
                };
                let h_word = num_to_words(h_val);

                if m == 0 {
                    match period {
                        Some(p) => format!("{h_word} {p}"),
                        None => format!("{h_word} o'clock"),
                    }
                } else if m < 10 {
                    let m_str = num_to_words(m);
                    match period {
                        Some(p) => format!("{h_word} oh {m_str} {p}"),
                        None => format!("{h_word} oh {m_str}"),
                    }
                } else {
                    let m_str = num_to_words(m).replace('-', " ");
                    match period {
                        Some(p) => format!("{h_word} {m_str} {p}"),
                        None => format!("{h_word} {m_str}"),
                    }
                }
            })
            .to_string()
    }

    /// Convert `in 2026` → `in twenty twenty-six`, `since 1999` → `since nineteen ninety-nine`.
    pub fn normalize_temporal_years(text: &str) -> String {
        re_temporal_year()
            .replace_all(text, |caps: &regex::Captures<'_>| {
                let prep = &caps[1];
                let year_num: u32 = caps[2].parse().unwrap_or(0);
                format!("{prep} {}", year_to_words(year_num))
            })
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dollar_amount() {
        let r = TextNormalizer::normalize("earn $1,000 today");
        assert!(r.contains("1000 dollars"), "got: {r}");
    }

    #[test]
    fn normalizes_rupee_amount() {
        let r = TextNormalizer::normalize("costs ₹50,000");
        assert!(r.contains("50000 rupees"), "got: {r}");
    }

    #[test]
    fn normalizes_k_suffix() {
        let r = TextNormalizer::normalize("has 2K followers");
        assert!(r.contains("2000"), "got: {r}");
    }

    #[test]
    fn normalizes_m_suffix() {
        let r = TextNormalizer::normalize("1.5M downloads");
        assert!(r.contains("1500000"), "got: {r}");
    }

    #[test]
    fn normalizes_percent() {
        let r = TextNormalizer::normalize("CPU at 87%");
        assert!(r.contains("87 percent"), "got: {r}");
    }

    #[test]
    fn strips_comma_thousands() {
        let r = TextNormalizer::normalize("value is 1,000,000");
        assert!(r.contains("1000000"), "got: {r}");
    }

    #[test]
    fn normalizes_urls() {
        let r = TextNormalizer::normalize("Visit https://voxforge.org/api today");
        assert!(r.contains("voxforge dot org slash api"), "got: {r}");
    }

    #[test]
    fn normalizes_emails() {
        let r = TextNormalizer::normalize("Email user@domain.com for support");
        assert!(r.contains("user at domain dot com"), "got: {r}");
    }

    #[test]
    fn normalizes_clock_times() {
        let r = TextNormalizer::normalize("Meeting set for 3:30 PM sharp");
        assert!(r.contains("three thirty PM"), "got: {r}");
    }

    #[test]
    fn normalizes_temporal_years() {
        let r = TextNormalizer::normalize("Built in 2026 and trusted since 1999");
        assert!(r.contains("in twenty twenty-six"), "got: {r}");
        assert!(r.contains("since nineteen ninety-nine"), "got: {r}");
    }

    #[test]
    fn leaves_regular_text_unchanged() {
        let input = "Hello world";
        assert_eq!(TextNormalizer::normalize(input), input);
    }

    #[test]
    fn test_humanize_cadence_no_double_commas() {
        let input = "we worked very hard on the engine, because we wanted to finish on time";
        let res = TextNormalizer::humanize_cadence(input);
        assert!(!res.contains(",,"), "Must not create double commas: {res}");
        assert!(
            !res.contains("because,"),
            "Comma should not be placed after conjunction: {res}"
        );
        assert!(
            res.contains("engine, because"),
            "Comma should remain before conjunction: {res}"
        );
    }

    #[test]
    fn normalizes_multilingual_abbreviations_brackets_and_markdown() {
        let raw = "Dr. **Smith** waited [pause 500ms] for 20~30 minutes &amp; more!!!!";
        let out = TextNormalizer::normalize_for_language(raw, Some("en-US"));
        assert!(out.contains("Doctor Smith"), "got: {out}");
        assert!(out.contains("[pause 500ms]"), "bracket preserved: {out}");
        assert!(out.contains("20 to 30"), "tilde range normalized: {out}");
        assert!(out.contains("and"), "HTML entity decoded: {out}");
        assert!(!out.contains("**"), "markdown stripped: {out}");
        assert!(!out.contains("!!!!"), "punctuation capped: {out}");

        // ZWNJ in Malayalam chillu compound stays intact & numbers verbalize cleanly
        let ml = "അവൻ\u{200C}ക്ക് 25% ലാഭം 10~20";
        let ml_out = TextNormalizer::normalize_for_language(ml, Some("ml-IN"));
        assert!(ml_out.contains('\u{200C}'), "ZWNJ preserved: {ml_out}");
        assert!(ml_out.contains("ഇരുപത്തിയഞ്ച് ശതമാനം"), "ml pct: {ml_out}");
        assert!(ml_out.contains("പത്ത് മുതൽ ഇരുപത്"), "ml range: {ml_out}");

        // Collapse 6+ repetition loops
        let loopy = "hello wait wait wait wait wait wait world";
        assert_eq!(
            TextNormalizer::collapse_repetitive_artifacts(loopy),
            "hello wait world"
        );
    }
}
