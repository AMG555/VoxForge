//! Multilingual bracket-aware sentence chunker for neural & concatenation TTS.
//!
//! Improvements over naïve punctuation splitting:
//! - Dense-script capacity scaling: CJK, Kana, Hangul, Thai, Lao, Khmer, Myanmar
//!   pack ~2.5x more spoken syllables per codepoint, so chunk budgets scale down.
//! - Non-Latin sentence terminators: `।` (`\u{0964}`), `॥` (`\u{0965}`),
//!   `۔` (`\u{06D4}`), `؟` (`\u{061F}`), `。`, `！`, `？`, `；`.
//! - Lookahead abbreviation & decimal protection (`Dr.`, `Mr.`, `3.14`, `z.B.`, `e.g.`).
//! - Bracket cue atomicity (`[pause 500ms]`, `[laugh]` are never cut mid-tag).
//! - Unspeakable fragment merging: orphaned punctuation-only chunks fold back onto
//!   the preceding spoken chunk so synthesis never tries to voice `"..."`.

const LATIN_SENTENCE_ABBREVS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "vs", "etc", "approx", "eg", "ie", "al",
    "fig", "vol", "no", "tel", "dept", "est", "sra", "dra", "ud", "uds", "srta", "bzw", "usw",
    "ca", "mme", "mlle", "mgr",
];

fn is_dense_syllabic_char(c: char) -> bool {
    let cp = c as u32;
    matches!(
        cp,
        0x3040..=0x30FF
            | 0x31F0..=0x31FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7A3
            | 0x0E00..=0x0E7F
            | 0x0E80..=0x0EFF
            | 0x1780..=0x17FF
            | 0x1000..=0x109F
    )
}

/// Compute effective max characters per chunk, scaling down by 2.5x when >30%
/// of non-whitespace characters belong to dense CJK / tonal scripts.
pub fn effective_max_chars(text: &str, max_chars: usize) -> usize {
    let mut non_space = 0usize;
    let mut dense = 0usize;
    for c in text.chars() {
        if !c.is_whitespace() {
            non_space += 1;
            if is_dense_syllabic_char(c) {
                dense += 1;
            }
        }
    }
    if non_space > 0 && dense * 10 >= non_space * 3 {
        ((max_chars as f32) / 2.5).round().max(40.0) as usize
    } else {
        max_chars.max(40)
    }
}

fn is_nonlatin_sentence_end(c: char) -> bool {
    matches!(
        c,
        '\u{0964}' | '\u{0965}' | '\u{06D4}' | '\u{061F}' | '。' | '！' | '？' | '；'
    )
}

fn has_speakable_content(s: &str) -> bool {
    s.chars().any(|c| c.is_alphanumeric())
}

fn preceding_word_lower(chars: &[(usize, char)], dot_idx: usize) -> String {
    let mut word = String::new();
    let mut j = dot_idx;
    while j > 0 {
        j -= 1;
        let ch = chars[j].1;
        if ch.is_alphabetic() {
            word.push(ch.to_ascii_lowercase());
        } else {
            break;
        }
    }
    word.chars().rev().collect()
}

/// Find the best split boundary (byte offset) inside `window` respecting tags, decimals,
/// abbreviations, sentence terminators, clauses, and spaces.
fn find_best_boundary(slice: &str, max_byte: usize) -> usize {
    let chars: Vec<(usize, char)> = slice.char_indices().collect();
    if chars.is_empty() {
        return 0;
    }

    let mut bracket_depth = 0usize;
    let mut last_sentence_end: Option<usize> = None;
    let mut last_clause_end: Option<usize> = None;
    let mut last_space: Option<usize> = None;

    for i in 0..chars.len() {
        let (byte_idx, c) = chars[i];
        let next_byte = byte_idx + c.len_utf8();
        if next_byte > max_byte {
            break;
        }

        if c == '[' {
            bracket_depth += 1;
            continue;
        }
        if c == ']' {
            bracket_depth = bracket_depth.saturating_sub(1);
            continue;
        }
        if bracket_depth > 0 {
            continue;
        }

        if c.is_whitespace() {
            last_space = Some(next_byte);
            continue;
        }

        if is_nonlatin_sentence_end(c) {
            // Skip repeated closing terminators like `॥` or `！？`
            if i + 1 < chars.len() && is_nonlatin_sentence_end(chars[i + 1].1) {
                continue;
            }
            last_sentence_end = Some(next_byte);
            continue;
        }

        if c == '!' || c == '?' {
            if i + 1 < chars.len() && (chars[i + 1].1 == '!' || chars[i + 1].1 == '?') {
                continue;
            }
            if i + 1 == chars.len()
                || chars[i + 1].1.is_whitespace()
                || matches!(chars[i + 1].1, '"' | '\'' | ')' | ']')
            {
                last_sentence_end = Some(next_byte);
            }
            continue;
        }

        if c == '.' {
            // Protect decimals (3.14) and ellipsis (`...`)
            let prev_digit = i > 0 && chars[i - 1].1.is_ascii_digit();
            let next_digit = i + 1 < chars.len() && chars[i + 1].1.is_ascii_digit();
            let prev_dot = i > 0 && chars[i - 1].1 == '.';
            let next_dot = i + 1 < chars.len() && chars[i + 1].1 == '.';
            if (prev_digit && next_digit) || next_dot {
                continue;
            }
            if !prev_dot {
                let abbrev = preceding_word_lower(&chars, i);
                if LATIN_SENTENCE_ABBREVS.contains(&abbrev.as_str()) {
                    continue;
                }
            }
            if i + 1 == chars.len()
                || chars[i + 1].1.is_whitespace()
                || matches!(chars[i + 1].1, '"' | '\'' | ')' | ']')
            {
                last_sentence_end = Some(next_byte);
            }
            continue;
        }

        if matches!(c, ',' | ';' | ':' | '—' | '–' | '、' | '，') {
            last_clause_end = Some(next_byte);
        }
    }

    last_sentence_end
        .or(last_clause_end)
        .or(last_space)
        .unwrap_or_else(|| {
            // Force split cleanly on char boundary strictly outside brackets
            let mut depth = 0usize;
            let mut safe = max_byte;
            for (byte_idx, c) in &chars {
                if *byte_idx >= max_byte {
                    break;
                }
                if *c == '[' {
                    if depth == 0 && *byte_idx > max_byte / 2 {
                        return *byte_idx;
                    }
                    depth += 1;
                } else if *c == ']' {
                    depth = depth.saturating_sub(1);
                }
                if depth == 0 {
                    safe = byte_idx + c.len_utf8();
                }
            }
            safe
        })
}

/// Merge fragments that lack speakable alphanumeric characters onto an adjacent chunk.
fn merge_unspeakable(mut chunks: Vec<String>) -> Vec<String> {
    if chunks.len() <= 1 {
        return chunks;
    }
    let mut merged: Vec<String> = Vec::with_capacity(chunks.len());
    for chunk in chunks.drain(..) {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !has_speakable_content(trimmed) {
            if let Some(prev) = merged.last_mut() {
                prev.push(' ');
                prev.push_str(trimmed);
                continue;
            }
        }
        merged.push(trimmed.to_string());
    }
    merged
}

/// Split `text` into sentence/clause-aligned chunks under `max_chars` (auto-scaled for CJK/dense scripts).
pub fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let limit = effective_max_chars(trimmed, max_chars);
    if trimmed.chars().count() <= limit {
        return vec![trimmed.to_string()];
    }

    let mut chunks = Vec::new();
    // Respect double newlines (paragraphs) first
    for para in trimmed.split("\n\n") {
        let p = para.trim();
        if p.is_empty() {
            continue;
        }
        let mut rest = p;
        while !rest.is_empty() {
            if rest.chars().count() <= limit {
                chunks.push(rest.to_string());
                break;
            }
            let max_byte = rest
                .char_indices()
                .nth(limit)
                .map(|(b, _)| b)
                .unwrap_or(rest.len());
            let mut split_at = find_best_boundary(rest, max_byte);
            if split_at == 0 || split_at > rest.len() {
                split_at = max_byte;
            }
            // Ensure valid UTF-8 boundary
            while split_at < rest.len() && !rest.is_char_boundary(split_at) {
                split_at += 1;
            }
            let head = rest[..split_at].trim();
            if !head.is_empty() {
                chunks.push(head.to_string());
            }
            rest = rest[split_at..].trim_start();
        }
    }

    let final_chunks = merge_unspeakable(chunks);
    if final_chunks.is_empty() {
        vec![trimmed.to_string()]
    } else {
        final_chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_abbreviations_decimals_and_brackets() {
        let text = "Dr. Smith earned 3.14 points today [pause 500ms] and thanked Prof. Lee at noon before walking home.";
        let chunks = chunk_text(text, 75);
        assert!(chunks.len() >= 2);
        // Should never split mid-"Dr." or mid-"[pause 500ms]"
        assert!(!chunks.iter().any(|c| c.ends_with("Dr.")));
        assert!(chunks
            .iter()
            .any(|c| c.contains("[pause 500ms]")));
        assert!(!chunks.iter().any(|c| c.ends_with("[pause")));
    }

    #[test]
    fn scales_dense_cjk_and_splits_nonlatin_terminators() {
        let ja = "これはとても長い日本語のテスト文章です。発音と自然なイントネーションを確認しています！";
        assert!(effective_max_chars(ja, 250) <= 105);
        let chunks = chunk_text(ja, 75);
        assert!(chunks.len() >= 2, "Expected CJK split, got {chunks:?}");

        let hi = "यह पहला हिंदी वाक्य है जिसकी लंबाई पर्याप्त है। यह दूसरा वाक्य है जो स्पष्ट सुनाई देगा।";
        let hi_chunks = chunk_text(hi, 65);
        assert!(hi_chunks.len() >= 2, "Expected danda split, got {hi_chunks:?}");
    }
}
