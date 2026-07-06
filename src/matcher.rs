// src/matcher.rs

pub struct MatchResult {
    pub original_idx: usize,
    pub score: i32,
    pub highlight_indices: Vec<usize>,
}

// 修改：接收 pinyin_cache 参数
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
        // 优先级 1: 原文 Fuzzy 匹配 (带高亮)
        if let Some(res) = fuzzy_match(item, query) {
            results.push(MatchResult {
                original_idx: idx,
                score: res.0 + 1000,
                highlight_indices: res.1,
            });
            continue;
        }

        // 优先级 2: 拼音匹配 (使用缓存数据)
        let (full, initials) = &pinyin_cache[idx];
        if let Some(res) = match_pinyin_cached(full, initials, query) {
            results.push(MatchResult {
                original_idx: idx,
                score: res.0,
                highlight_indices: res.1,
            });
        }
    }

    results.sort_by(|a, b| b.score.cmp(&a.score));
    results
}

pub fn fuzzy_match(text: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    let text_chars: Vec<char> = text.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();

    let mut text_idx = 0;
    let mut query_idx = 0;
    let mut highlights = Vec::new();
    let mut score = 0;

    while text_idx < text_chars.len() && query_idx < query_chars.len() {
        if text_chars[text_idx].eq_ignore_ascii_case(&query_chars[query_idx]) {
            if !highlights.is_empty() && highlights.last() == Some(&(text_idx - 1)) {
                score += 20; // 连续匹配奖励
            } else {
                score += 10;
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

// 新增：基于缓存的拼音匹配函数
fn match_pinyin_cached(full: &str, initials: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    let q = query.to_lowercase();

    if !initials.is_empty() {
        if let Some(res) = fuzzy_match(initials, &q) {
            return Some((res.0 - 10, res.1));
        }
    }

    if !full.is_empty() {
        if let Some(res) = fuzzy_match(full, &q) {
            return Some((res.0 - 20, res.1));
        }
    }

    None
}
