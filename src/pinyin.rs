use pinyin::ToPinyin;

// 获取字符串的全拼连写 (例如 "计算机" -> "jisuanji")
pub fn get_full_pinyin(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        if let Some(p) = c.to_pinyin() {
            result.push_str(p.plain());
        }
    }
    result
}

// 获取字符串的拼音首字母连写 (例如 "计算机" -> "jsj")
pub fn get_initials(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        if let Some(p) = c.to_pinyin() {
            // 从 plain() 里取第一个字符
            if let Some(first_char) = p.plain().chars().next() {
                result.push(first_char);
            }
        }
    }
    result
}

// 匹配逻辑：先尝试首字母匹配，再尝试全拼匹配
pub fn match_pinyin(text: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    let q = query.to_lowercase();

    // 优先级 1: 首字母匹配
    let initials = get_initials(text);
    if !initials.is_empty() {
        if let Some(res) = crate::matcher::fuzzy_match(&initials, &q) {
            return Some((res.0 - 10, res.1));
        }
    }

    // 优先级 2: 全拼匹配
    let full = get_full_pinyin(text);
    if !full.is_empty() {
        if let Some(res) = crate::matcher::fuzzy_match(&full, &q) {
            return Some((res.0 - 20, res.1));
        }
    }

    None
}

// 新增：单次遍历获取全拼和首字母
pub fn get_pinyin_pair(text: &str) -> (String, String) {
    let mut full = String::new();
    let mut initials = String::new();
    for c in text.chars() {
        if let Some(p) = c.to_pinyin() {
            full.push_str(p.plain());
            if let Some(first) = p.plain().chars().next() {
                initials.push(first);
            }
        }
    }
    (full, initials)
}
