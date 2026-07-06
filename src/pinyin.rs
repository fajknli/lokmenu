use pinyin::ToPinyinMulti;

// 获取字符串的全拼连写，包含多音字 (例如 "行" -> "hangxing")
pub fn get_full_pinyin(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        if let Some(multi) = c.to_pinyin_multi() {
            // 将所有可能的读音拼接在一起
            for p in multi {
                result.push_str(p.plain());
            }
        }
    }
    result
}

// 获取字符串的拼音首字母连写，包含多音字 (例如 "行" -> "hx")
pub fn get_initials(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        if let Some(multi) = c.to_pinyin_multi() {
            // 将所有可能读音的首字母拼接
            for p in multi {
                if let Some(first_char) = p.plain().chars().next() {
                    result.push(first_char);
                }
            }
        }
    }
    result
}

pub fn match_pinyin(text: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    let q = query.to_lowercase();

    let initials = get_initials(text);
    if !initials.is_empty() {
        if let Some(res) = crate::matcher::fuzzy_match(&initials, &q) {
            return Some((res.0 - 10, res.1));
        }
    }

    let full = get_full_pinyin(text);
    if !full.is_empty() {
        if let Some(res) = crate::matcher::fuzzy_match(&full, &q) {
            return Some((res.0 - 20, res.1));
        }
    }

    None
}

// 新增：单次遍历获取全拼和首字母（包含多音字）
pub fn get_pinyin_pair(text: &str) -> (String, String) {
    let mut full = String::new();
    let mut initials = String::new();
    for c in text.chars() {
        if let Some(multi) = c.to_pinyin_multi() {
            for p in multi {
                full.push_str(p.plain());
                if let Some(first) = p.plain().chars().next() {
                    initials.push(first);
                }
            }
        }
    }
    (full, initials)
}
