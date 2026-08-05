// src/state.rs

use std::collections::HashMap;
use crate::config::Config;
use crate::matcher;
use std::collections::HashSet;

pub struct State {
    pub items: Vec<String>,
    pub query: String,
    // 新增：光标位置，基于字符计数，而非字节
    pub cursor_pos: usize,
    pub pinyin_cache: HashMap<usize, crate::pinyin::PinyinData>,
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

impl State {
    pub fn new(items: Vec<String>, config: &Config) -> Self {
        let mut pinyin_cache = HashMap::with_capacity(items.len());
        for (idx, item) in items.iter().enumerate() {
            let has_cjk = item.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
            if has_cjk {
                pinyin_cache.insert(idx, crate::pinyin::get_pinyin_pair(item));
            }
        }

        let mut state = Self {
            items,
            pinyin_cache,
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

    pub fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered_items = (0..self.items.len()).collect();
            self.highlights = vec![HashSet::new(); self.items.len()];
        } else {
            let items: Vec<&str> = self.items.iter().map(|s| s.as_str()).collect();
            let results = matcher::filter(&items, &self.pinyin_cache, &self.query);
            self.filtered_items = results.iter().map(|r| r.original_idx).collect();
            self.highlights = results.iter().map(|r| r.highlight_indices.clone()).collect();
        }

        if self.selected_idx >= self.filtered_items.len() {
            self.selected_idx = 0;
        }
        self.adjust_scroll();
        self.need_redraw = true;
    }

    // 修改：在光标处插入字符
    pub fn push_char(&mut self, c: char) {
        let mut chars: Vec<char> = self.query.chars().collect();
        chars.insert(self.cursor_pos, c);
        self.query = chars.into_iter().collect();
        self.cursor_pos += 1;
        self.update_filter();
    }

    // 修改：删除光标前的一个字符
    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            let mut chars: Vec<char> = self.query.chars().collect();
            chars.remove(self.cursor_pos - 1);
            self.query = chars.into_iter().collect();
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

    // 修改：向左删除一个单词
    pub fn delete_word_left(&mut self) {
        if self.cursor_pos == 0 { return; }
        let mut chars: Vec<char> = self.query.chars().collect();

        // 跳过空格
        while self.cursor_pos > 0 && chars[self.cursor_pos - 1].is_whitespace() {
            chars.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
        }
        // 删除单词
        while self.cursor_pos > 0 && !chars[self.cursor_pos - 1].is_whitespace() {
            chars.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
        }

        self.query = chars.into_iter().collect();
        self.update_filter();
    }

    // 新增：删到行首
    pub fn delete_to_start(&mut self) {
        if self.cursor_pos > 0 {
            let mut chars: Vec<char> = self.query.chars().collect();
            chars.drain(..self.cursor_pos);
            self.query = chars.into_iter().collect();
            self.cursor_pos = 0;
            self.update_filter();
        }
    }

    // 新增：删到行尾
    pub fn delete_to_end(&mut self) {
        let mut chars: Vec<char> = self.query.chars().collect();
        if self.cursor_pos < chars.len() {
            chars.truncate(self.cursor_pos);
            self.query = chars.into_iter().collect();
            self.update_filter();
        }
    }

    // 修改：处理输入法提交的字符串
    pub fn commit_str(&mut self, s: &str) {
        let mut chars: Vec<char> = self.query.chars().collect();
        let s_chars: Vec<char> = s.chars().collect();

        for c in s_chars {
            chars.insert(self.cursor_pos, c);
            self.cursor_pos += 1;
        }

        self.query = chars.into_iter().collect();
        self.preedit.clear();
        self.update_filter();
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
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.need_redraw = true;
        }
    }

    pub fn cursor_right(&mut self) {
        if self.cursor_pos < self.query.chars().count() {
            self.cursor_pos += 1;
            self.need_redraw = true;
        }
    }

    pub fn cursor_start(&mut self) {
        if self.cursor_pos != 0 {
            self.cursor_pos = 0;
            self.need_redraw = true;
        }
    }

    pub fn cursor_end(&mut self) {
        let len = self.query.chars().count();
        if self.cursor_pos != len {
            self.cursor_pos = len;
            self.need_redraw = true;
        }
    }

    // --- 列表移动方法 ---
    pub fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            self.adjust_scroll();
            self.need_redraw = true;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_idx + 1 < self.filtered_items.len() {
            self.selected_idx += 1;
            self.adjust_scroll();
            self.need_redraw = true;
        }
    }

    pub fn move_down_by(&mut self, n: usize) {
        if self.filtered_items.is_empty() { return; }
        self.selected_idx = (self.selected_idx + n).min(self.filtered_items.len() - 1);
        self.adjust_scroll();
        self.need_redraw = true;
    }

    pub fn move_up_by(&mut self, n: usize) {
        if self.filtered_items.is_empty() { return; }
        self.selected_idx = self.selected_idx.saturating_sub(n);
        self.adjust_scroll();
        self.need_redraw = true;
    }

    pub fn toggle_mark(&mut self) {
        if let Some(&idx) = self.filtered_items.get(self.selected_idx) {
            if !self.marked_items.insert(idx) {
                self.marked_items.remove(&idx);
            }
            self.need_redraw = true;
        }
    }

    fn adjust_scroll(&mut self) {
        let visible = self.visible_lines as usize;
        if self.filtered_items.is_empty() {
            self.scroll_offset = 0;
            return;
        }
        if self.selected_idx < self.scroll_offset {
            self.scroll_offset = self.selected_idx;
        } else if self.selected_idx >= self.scroll_offset + visible {
            self.scroll_offset = self.selected_idx - visible + 1;
        }
    }

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
            self.output = Some(self.items[idx].clone());
            self.output_indices = vec![idx];
            self.selected_original_idx = Some(idx);
        } else {
            self.output = Some(self.query.clone());
            self.output_indices = Vec::new();
            self.selected_original_idx = Some(usize::MAX);
        }
        self.exit_code = Some(0);
    }

    pub fn cancel(&mut self) {
        self.exit_code = Some(1);
    }
}
