// src/state.rs

use std::collections::HashSet;
use crate::config::Config;
use crate::matcher;

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
    pub marked_items: HashSet<usize>, // 新增：记录被标记的原始索引
    pub output: Option<String>,
    pub selected_original_idx: Option<usize>,
    pub exit_code: Option<i32>,
    pub need_redraw: bool,
}

impl State {
    pub fn new(items: Vec<String>, config: &Config) -> Self {
        let cached_pinyin = items.iter().map(|s| {
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
            output: None,
            selected_original_idx: None,
            exit_code: None,
            need_redraw: true,
        };
        state.update_filter();
        state
    }

    pub fn update_filter(&mut self) {
        let items: Vec<&str> = self.items.iter().map(|s| s.as_str()).collect();
        let results = matcher::filter(&items, &self.cached_pinyin, &self.query);

        self.filtered_items = results.iter().map(|r| r.original_idx).collect();
        self.highlights = results.iter().map(|r| r.highlight_indices.clone()).collect();

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
        // 1. 只要开启了多选模式，并且标记列表不为空，就【只】返回被标记的项
        // 完全无视当前搜索框内容，完全无视当前光标位置
        if multi_select && !self.marked_items.is_empty() {
            let mut result: Vec<String> = Vec::new();
            // 遍历原始全量 items，保证输出顺序稳定
            for (idx, item) in self.items.iter().enumerate() {
                if self.marked_items.contains(&idx) {
                    result.push(item.clone());
                }
            }

            self.output = Some(result.join("\n"));
            self.selected_original_idx = Some(usize::MAX);
            self.exit_code = Some(0);
            return; // 直接结束，不走下面的逻辑
        }

        // 2. 如果是多选模式但没有标记任何项，或者单选模式，走正常单选/回显逻辑
        if let Some(&idx) = self.filtered_items.get(self.selected_idx) {
            // 输出当前光标选中
            self.output = Some(self.items[idx].clone());
            self.selected_original_idx = Some(idx);
        } else {
            // 未匹配任何项，输出用户当前输入的内容
            self.output = Some(self.query.clone());
            self.selected_original_idx = Some(usize::MAX);
        }
        self.exit_code = Some(0);
    }

    pub fn cancel(&mut self) {
        self.exit_code = Some(1);
    }
}
