use pinyin::ToPinyinMulti;

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
