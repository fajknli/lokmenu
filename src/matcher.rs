// src/matcher.rs

const MAX_MATCHES: usize = 500;

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
    let q_chars: Vec<char> = query.chars().collect();

    for (idx, item) in items.iter().enumerate() {
        // 优先级 1: 原文匹配
        if let Some(res) = fuzzy_match(item, &q_chars) {
            results.push(MatchResult {
                original_idx: idx,
                score: res.0 + 1000,
                highlight_indices: res.1,
            });
            continue;
        }

        // 优先级 2: 拼音匹配 (跳过空缓存)
        let (full, initials) = &pinyin_cache[idx];
        if !initials.is_empty() || !full.is_empty() {
            if let Some(res) = match_pinyin_cached(full, initials, &q_chars) {
                results.push(MatchResult {
                    original_idx: idx,
                    score: res.0,
                    highlight_indices: res.1,
                });
            }
        }
    }

    // Top-K 截断：极速提取前 500 名 (原地截断，零额外内存分配)
    if results.len() > MAX_MATCHES {
        results.select_nth_unstable_by(MAX_MATCHES - 1, |a, b| b.score.cmp(&a.score));
        results.truncate(MAX_MATCHES);
        results.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    } else {
        results.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    }

    results
}

pub fn fuzzy_match(text: &str, q_chars: &[char]) -> Option<(i32, Vec<usize>)> {
    let mut text_idx: usize = 0;
    let mut query_idx: usize = 0;
    let mut highlights = Vec::new();
    let mut score: i32 = 0;
    let mut prev_char: Option<char> = None;

    for c in text.chars() {
        if query_idx >= q_chars.len() { break; }

        if c.eq_ignore_ascii_case(&q_chars[query_idx]) {
            // 1. 连续性
            if highlights.is_empty() {
                // 首个匹配，无连续/断开概念
            } else if matches!(highlights.last(), Some(&h) if h + 1 == text_idx) {
                score += 20; // 连续奖励
            } else {
                score -= 10; // 断开惩罚
            }

            // 2. 大小写精确
            if c == q_chars[query_idx] {
                score += 5;
            }

            // 3. 边界奖励：路径分隔符、单词分隔符之后的首字符
            let is_boundary = text_idx == 0 || matches!(
                prev_char,
                Some('/' | '\\' | '-' | '_' | '.' | ' ')
            );
            if is_boundary {
                score += 30;
            }

            highlights.push(text_idx);
            query_idx += 1;
        }
        prev_char = Some(c);
        text_idx += 1;
    }

    if query_idx == q_chars.len() {
        Some((score, highlights))
    } else {
        None
    }
}

fn match_pinyin_cached(full: &str, initials: &str, q_chars: &[char]) -> Option<(i32, Vec<usize>)> {
    if !initials.is_empty() {
        if let Some(res) = fuzzy_match(initials, q_chars) {
            return Some((res.0 - 10, res.1));
        }
    }

    if !full.is_empty() {
        if let Some(res) = fuzzy_match(full, q_chars) {
            return Some((res.0 - 20, res.1));
        }
    }

    None
}
