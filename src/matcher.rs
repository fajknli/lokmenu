// src/matcher.rs

pub struct MatchResult {
    pub original_idx: usize,
    pub score: i32,
    pub highlight_indices: Vec<usize>,
}

pub fn filter(items: &[&str], pinyin_cache: &[(String, String)], query: &str) -> Vec<MatchResult> {
    if query.is_empty() {
        return items
            .iter()
            .enumerate()
            .map(|(idx, _)| MatchResult {
                original_idx: idx,
                score: 0,
                highlight_indices: Vec::new(),
            })
            .collect();
    }

    let mut results = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        if let Some(res) = fuzzy_match(item, query) {
            results.push(MatchResult {
                original_idx: idx,
                score: res.0 + 1000,
                highlight_indices: res.1,
            });
            continue;
        }

        let (full, initials) = &pinyin_cache[idx];
        // 优化：直接传 query，fuzzy_match 内部已经用 eq_ignore_ascii_case 处理了大小写
        if let Some(res) = match_pinyin_cached(full, initials, query) {
            results.push(MatchResult {
                original_idx: idx,
                score: res.0,
                highlight_indices: res.1,
            });
        }
    }

    results.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    results
}

pub fn fuzzy_match(text: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    let query_chars: Vec<char> = query.chars().collect();

    let mut text_idx = 0;
    let mut query_idx = 0;
    let mut highlights = Vec::new();
    let mut score = 0;

    // 优化：直接遍历字符，不分配 Vec<char>
    for c in text.chars() {
        if query_idx >= query_chars.len() { break; }

        if c.eq_ignore_ascii_case(&query_chars[query_idx]) {
            if !highlights.is_empty() && highlights.last() == Some(&(text_idx - 1)) {
                score += 20;
            } else {
                score += 10;
            }

            if c == query_chars[query_idx] {
                score += 5;
            }

            highlights.push(text_idx);
            query_idx += 1;
        }
        text_idx += 1;
    }

    if query_idx == query_chars.len() {
        Some((score, highlights))
    } else {
        None
    }
}

fn match_pinyin_cached(full: &str, initials: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    if !initials.is_empty() {
        if let Some(res) = fuzzy_match(initials, query) {
            return Some((res.0 - 10, res.1));
        }
    }

    if !full.is_empty() {
        if let Some(res) = fuzzy_match(full, query) {
            return Some((res.0 - 20, res.1));
        }
    }

    None
}
