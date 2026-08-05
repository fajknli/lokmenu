// src/state.rs

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use crate::config::Config;
use crate::matcher;

pub struct State {
    pub items: Vec<String>,
    pub query: String,
    pub cursor_pos: usize,
    pub pinyin_cache: HashMap<usize, crate::pinyin::PinyinData>,
    pub history: HashMap<String, u32>, // 新增：历史记录
    pub preedit: String,
    pub filtered_items: Vec<usize>,
    pub highlights: Vec<HashSet<usize>>,
    pub selected_idx: usize,
    pub scroll_offset: usize,
    pub visible_lines: u32,
    pub marked_items: HashSet<usize>,
    pub null_sep: bool,
    pub output: Option<String>,
    pub output_indices: Vec<usize>,
    pub selected_original_idx: Option<usize>,
    pub exit_code: Option<i32>,
    pub need_redraw: bool,
}

// 辅助函数：获取历史记录文件路径
fn get_history_path() -> Option<PathBuf> {
    let base = if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache")
    } else {
        return None;
    };
    Some(base.join("lokmenu_history"))
}

impl State {
    pub fn new(items: Vec<String>, config: &Config) -> Self {
        let mut pinyin_cache = HashMap::with_capacity(items.len());
        for (idx, item) in items.iter().enumerate() {
            let has_cjk = item.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
            if has_cjk {
                pinyin_cache.insert(idx, crate::pinyin::get_pinyin_pair(item));
            }
        }

        let mut history = HashMap::new();
        if let Some(path) = get_history_path() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                for line in content.lines() {
                    if let Some((count, name)) = line.split_once('\t') {
                        if let Ok(c) = count.parse::<u32>() {
                            history.insert(name.to_string(), c);
                        }
                    }
                }
            }
        }

        let mut state = Self {
            items,
            pinyin_cache,
            history,
            query: String::new(),
            cursor_pos: 0,
            preedit: String::new(),
            filtered_items: Vec::new(),
            highlights: Vec::new(),
            selected_idx: 0,
            scroll_offset: 0,
            visible_lines: config.lines,
            marked_items: HashSet::new(),
            null_sep: config.null,
            output: None,
            output_indices: Vec::new(),
            selected_original_idx: None,
            exit_code: None,
            need_redraw: true,
        };
        state.update_filter();
        state
    }

    // 替换 update_filter 方法
    pub fn update_filter(&mut self) {
        if self.query.is_empty() {
            let mut indexed_items: Vec<(usize, &String)> = self.items.iter().enumerate().collect();
            indexed_items.sort_by(|a, b| {
                let h_a = self.history.get(a.1).map_or(0, |&c| c);
                let h_b = self.history.get(b.1).map_or(0, |&c| c);
                // 先按频率降序，频率相同按字母升序，保证每次打开列表顺序稳定
                h_b.cmp(&h_a).then_with(|| a.1.cmp(b.1))
            });
            self.filtered_items = indexed_items.iter().map(|(i, _)| *i).collect();
            self.highlights = vec![HashSet::new(); self.items.len()];
        } else {
            let items: Vec<&str> = self.items.iter().map(|s| s.as_str()).collect();
            let results = matcher::filter(&items, &self.pinyin_cache, &self.history, &self.query);
            self.filtered_items = results.iter().map(|r| r.original_idx).collect();
            self.highlights = results.iter().map(|r| r.highlight_indices.clone()).collect();
        }

        if self.selected_idx >= self.filtered_items.len() {
            self.selected_idx = 0;
        }
        self.adjust_scroll();
        self.need_redraw = true;
    }

    // 替换 select_current 方法
    pub fn select_current(&mut self, multi_select: bool) {
        if multi_select && !self.marked_items.is_empty() {
            let mut result: Vec<String> = Vec::new();
            let mut indices = Vec::new();
            for (idx, item) in self.items.iter().enumerate() {
                if self.marked_items.contains(&idx) {
                    result.push(item.clone());
                    indices.push(idx);
                }
            }
            let sep = if self.null_sep { "\0" } else { "\n" };
            self.output = Some(result.join(sep));
            self.output_indices = indices;
            self.selected_original_idx = Some(usize::MAX);
            self.exit_code = Some(0);
            return;
        }

        if let Some(&idx) = self.filtered_items.get(self.selected_idx) {
            let item_text = &self.items[idx];

            // 更新历史记录
            *self.history.entry(item_text.clone()).or_insert(0) += 1;

            // 准备写入文件的内容
            if let Some(path) = get_history_path() {
                // 清理膨胀：只保留使用次数最高的前 1000 条
                let mut entries: Vec<(String, u32)> = self.history.iter().map(|(k, &v)| (k.clone(), v)).collect();
                entries.sort_by(|a, b| b.1.cmp(&a.1)); // 按次数降序
                entries.truncate(1000);

                let content: String = entries.iter()
                    .map(|(k, v)| format!("{}\t{}", v, k))
                    .collect::<Vec<_>>()
                    .join("\n");

                // 异步写入硬盘，不阻塞主线程退出
                std::thread::spawn(move || {
                    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
                    let _ = std::fs::write(&path, content);
                });
            }

            self.output = Some(item_text.clone());
            self.output_indices = vec![idx];
            self.selected_original_idx = Some(idx);
        } else {
            self.output = Some(self.query.clone());
            self.output_indices = Vec::new();
            self.selected_original_idx = Some(usize::MAX);
        }
        self.exit_code = Some(0);
    }

    // 辅助函数：将字符索引转为字节索引
    fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
        s.char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or_else(|| s.len())
    }

    // 零分配在光标处插入字符
    pub fn push_char(&mut self, c: char) {
        let byte_idx = Self::char_to_byte_idx(&self.query, self.cursor_pos);
        self.query.insert(byte_idx, c);
        self.cursor_pos += 1;
        self.update_filter();
    }

    // 零分配删除光标前的一个字符
    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            let byte_idx = Self::char_to_byte_idx(&self.query, self.cursor_pos - 1);
            self.query.remove(byte_idx);
            self.cursor_pos -= 1;
            self.update_filter();
        }
    }

    pub fn clear_query(&mut self) {
        if !self.query.is_empty() {
            self.query.clear();
            self.cursor_pos = 0;
            self.update_filter();
        }
    }

    // 纯迭代器计算，零分配向左删除一个单词
    pub fn delete_word_left(&mut self) {
        if self.cursor_pos == 0 { return; }

        let byte_offset = Self::char_to_byte_idx(&self.query, self.cursor_pos);
        let mut start_byte_idx = 0;
        let mut found_word = false;

        // 使用 char_indices 倒序遍历，避开 Chars 不能 take().rev() 的问题
        let mut iter = self.query.char_indices().rev().take_while(|(i, _)| *i < byte_offset);

        for (i, c) in iter.by_ref() {
            if !c.is_whitespace() {
                found_word = true;
                start_byte_idx = i;
            } else if found_word {
                // 碰到空格说明单词结束，起始删除位置是空格的下一个字符
                start_byte_idx = i + 1;
                break;
            }
        }

        if found_word {
            self.query.drain(start_byte_idx..byte_offset);
            self.cursor_pos = self.query[..start_byte_idx].chars().count();
        } else {
            // 全是空格或者遇到行首
            self.query.drain(0..byte_offset);
            self.cursor_pos = 0;
        }
        self.update_filter();
    }

    // 零分配删到行首
    pub fn delete_to_start(&mut self) {
        if self.cursor_pos > 0 {
            let byte_idx = Self::char_to_byte_idx(&self.query, 0);
            let end_byte_idx = Self::char_to_byte_idx(&self.query, self.cursor_pos);
            self.query.drain(byte_idx..end_byte_idx);
            self.cursor_pos = 0;
            self.update_filter();
        }
    }

    // 零分配删到行尾
    pub fn delete_to_end(&mut self) {
        let char_len = self.query.chars().count();
        if self.cursor_pos < char_len {
            let byte_start = Self::char_to_byte_idx(&self.query, self.cursor_pos);
            self.query.drain(byte_start..);
            self.update_filter();
        }
    }

    // 零分配处理输入法提交的字符串
    pub fn commit_str(&mut self, s: &str) {
        if !s.is_empty() {
            let byte_idx = Self::char_to_byte_idx(&self.query, self.cursor_pos);
            self.query.insert_str(byte_idx, s);
            self.cursor_pos += s.chars().count();
            self.update_filter();
        }
        self.preedit.clear();
    }

    pub fn set_preedit(&mut self, s: &str) {
        self.preedit = s.to_string();
        self.need_redraw = true;
    }

    pub fn clear_preedit(&mut self) {
        self.preedit.clear();
        self.need_redraw = true;
    }

    // --- 光标移动方法 ---
    pub fn cursor_left(&mut self) {
        if self.cursor_pos > 0 { self.cursor_pos -= 1; self.need_redraw = true; }
    }
    pub fn cursor_right(&mut self) {
        if self.cursor_pos < self.query.chars().count() { self.cursor_pos += 1; self.need_redraw = true; }
    }
    pub fn cursor_start(&mut self) {
        if self.cursor_pos != 0 { self.cursor_pos = 0; self.need_redraw = true; }
    }
    pub fn cursor_end(&mut self) {
        let len = self.query.chars().count();
        if self.cursor_pos != len { self.cursor_pos = len; self.need_redraw = true; }
    }

    // --- 列表移动方法 ---
    pub fn move_up(&mut self) {
        if self.selected_idx > 0 { self.selected_idx -= 1; self.adjust_scroll(); self.need_redraw = true; }
    }
    pub fn move_down(&mut self) {
        if self.selected_idx + 1 < self.filtered_items.len() { self.selected_idx += 1; self.adjust_scroll(); self.need_redraw = true; }
    }
    pub fn move_down_by(&mut self, n: usize) {
        if self.filtered_items.is_empty() { return; }
        self.selected_idx = (self.selected_idx + n).min(self.filtered_items.len() - 1);
        self.adjust_scroll(); self.need_redraw = true;
    }
    pub fn move_up_by(&mut self, n: usize) {
        if self.filtered_items.is_empty() { return; }
        self.selected_idx = self.selected_idx.saturating_sub(n);
        self.adjust_scroll(); self.need_redraw = true;
    }

    pub fn toggle_mark(&mut self) {
        if let Some(&idx) = self.filtered_items.get(self.selected_idx) {
            if !self.marked_items.insert(idx) { self.marked_items.remove(&idx); }
            self.need_redraw = true;
        }
    }

    fn adjust_scroll(&mut self) {
        let visible = self.visible_lines as usize;
        if self.filtered_items.is_empty() { self.scroll_offset = 0; return; }
        if self.selected_idx < self.scroll_offset { self.scroll_offset = self.selected_idx; }
        else if self.selected_idx >= self.scroll_offset + visible { self.scroll_offset = self.selected_idx - visible + 1; }
    }

    pub fn cancel(&mut self) {
        self.exit_code = Some(1);
    }
}
