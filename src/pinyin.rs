// 未来的 v0.2 会在这里用 phf 引入完整的拼音字库
// 现在仅作逻辑占位演示

pub fn match_pinyin(text: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    // 模拟："计算机" 遇到 "jsj"
    if text.contains("计算机") && query == "jsj" {
        // 假设命中了第 0, 1, 2 个字符
        return Some((500, vec![0, 1, 2]));
    }

    // 模拟："jisuanji" 遇到 "jisji"
    if text.contains("计算机") && query == "jisji" {
        return Some((400, vec![0, 1, 2]));
    }

    None
}
