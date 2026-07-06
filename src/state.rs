// src/state.rs

use std::collections::HashSet;
use crate::config::Config;
use crate::matcher;
use rayon::prelude::*;

pub struct State {
    pub items: Vec<String>,
    pub cached_pinyin: Vec<(String, String)>,
    pub query: String,
    pub preedit: String,
    pub filtered_items: Vec<usize>,
    pub highlights: Vec<Vec<usize>>,
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
        let cached_pinyin = items.par_iter().map(|s| {
            crate::pinyin::get_pinyin_pair(s)
        }).collect();

        let mut state = Self {
            items,
            cached_pinyin,
            query: String::new(),
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
            // 空查询：直接用原始索引，不需要跑 matcher
            self.filtered_items = (0..self.items.len()).collect();
            self.highlights = vec![Vec::new(); self.items.len()];
        } else {
            let items: Vec<&str> = self.items.iter().map(|s| s.as_str()).collect();
            let results = matcher::filter(&items, &self.cached_pinyin, &self.query);
            self.filtered_items = results.iter().map(|r| r.original_idx).collect();
            self.highlights = results.iter().map(|r| r.highlight_indices.clone()).collect();
        }

        if self.selected_idx >= self.filtered_items.len() {
            self.selected_idx = 0;
        }
        self.adjust_scroll();
        self.need_redraw = true;
    }

    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_filter();
    }

    pub fn clear_query(&mut self) {
        if !self.query.is_empty() {
            self.query.clear();
            self.update_filter();
        }
    }

    pub fn delete_word(&mut self) {
        if self.query.is_empty() { return; }

        let mut chars: Vec<char> = self.query.chars().collect();
        while matches!(chars.last(), Some(c) if c.is_whitespace()) {
            chars.pop();
        }
        while !chars.is_empty() {
            if chars.last().map_or(false, |c| c.is_whitespace()) {
                break;
            }
            chars.pop();
        }

        self.query = chars.into_iter().collect();
        self.update_filter();
    }

    pub fn commit_str(&mut self, s: &str) {
        self.query.push_str(s);
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

    // 新增：批量向下移动
    pub fn move_down_by(&mut self, n: usize) {
        if self.filtered_items.is_empty() { return; }
        self.selected_idx = (self.selected_idx + n).min(self.filtered_items.len() - 1);
        self.adjust_scroll();
        self.need_redraw = true;
    }

    pub fn move_up_by(&mut self, n: usize) {
        if self.filtered_items.is_empty() { return; } // 增加空列表保护
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
            let mut indices = Vec::new(); // 新增
            for (idx, item) in self.items.iter().enumerate() {
                if self.marked_items.contains(&idx) {
                    result.push(item.clone());
                    indices.push(idx); // 新增
                }
            }

            let sep = if self.null_sep { "\0" } else { "\n" };
            self.output = Some(result.join(sep));
            self.output_indices = indices; // 新增
            self.selected_original_idx = Some(usize::MAX);
            self.exit_code = Some(0);
            return;
        }

        if let Some(&idx) = self.filtered_items.get(self.selected_idx) {
            self.output = Some(self.items[idx].clone());
            self.output_indices = vec![idx]; // 新增
            self.selected_original_idx = Some(idx);
        } else {
            self.output = Some(self.query.clone());
            self.output_indices = Vec::new(); // 新增
            self.selected_original_idx = Some(usize::MAX);
        }
        self.exit_code = Some(0);
    }

    pub fn cancel(&mut self) {
        self.exit_code = Some(1);
    }
}
