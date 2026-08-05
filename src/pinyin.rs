use pinyin::ToPinyinMulti;

// 新增：拼音数据结构，包含全拼、首字母以及它们到原文索引的映射表
#[derive(Clone, Debug, Default)]
pub struct PinyinData {
    pub full: String,
    pub init: String,
    pub full_map: Vec<usize>,
    pub init_map: Vec<usize>,
}

/// 单次遍历获取全拼和首字母（包含多音字），并建立字符到原文的索引映射
/// 1. 多音字之间插入隐形墙 '\x01'，防止跨读音匹配 (如 重庆 -> chong\x01qing，防止 qingchong 匹配)
/// 2. 非中文字符直接加入，保证混合字符串 (如 北京abc) 能正常匹配
pub fn get_pinyin_pair(text: &str) -> PinyinData {
    let mut full = String::new();
    let mut init = String::new();
    let mut full_map = Vec::new();
    let mut init_map = Vec::new();

    for (orig_idx, c) in text.chars().enumerate() {
        if let Some(multi) = c.to_pinyin_multi() {
            let mut is_first_pron = true;
            for p in multi {
                let plain = p.plain();

                // 多音字之间插入隐形墙
                if !is_first_pron {
                    full.push('\x01');
                    init.push('\x01');
                    // 隐形墙不是真实字符，用 usize::MAX 占位，matcher 里不会用到
                    full_map.push(usize::MAX);
                    init_map.push(usize::MAX);
                }

                for ch in plain.chars() {
                    full.push(ch);
                    full_map.push(orig_idx);
                }
                if let Some(first) = plain.chars().next() {
                    init.push(first);
                    init_map.push(orig_idx);
                }

                is_first_pron = false;
            }
        } else {
            // 非中文字符直接加入，保证可以正常匹配和高亮
            full.push(c);
            full_map.push(orig_idx);
            init.push(c);
            init_map.push(orig_idx);
        }
    }

    PinyinData { full, init, full_map, init_map }
}
