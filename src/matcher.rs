pub struct MatchResult {
    pub original_idx: usize,
    pub score: i32,
    pub highlight_indices: Vec<usize>,
}

pub fn filter(items: &[&str], query: &str) -> Vec<MatchResult> {
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

        // 优先级 2: 拼音匹配 (暂不处理高亮位置)
        if let Some(res) = crate::pinyin::match_pinyin(item, query) {
            results.push(MatchResult {
                original_idx: idx,
                score: res.0,
                highlight_indices: Vec::new(), // 暂时返回空
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
            highlights.push(text_idx);
            score += 10;
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
