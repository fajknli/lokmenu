// src/state.rs

use crate::config::Config;
use crate::matcher;

pub struct State {
    pub items: Vec<String>,
    pub query: String,
    pub preedit: String,
    pub filtered_items: Vec<usize>,
    pub highlights: Vec<Vec<usize>>, // 新增：保存每个项的高亮字符索引
    pub selected_idx: usize,
    pub scroll_offset: usize,         // 新增：当前滚动偏移
    pub visible_lines: u32,           // 新增：可视行数
    pub output: Option<String>,
    pub selected_original_idx: Option<usize>,
    pub exit_code: Option<i32>,
    pub need_redraw: bool,
}

impl State {
    pub fn new(items: Vec<String>, config: &Config) -> Self {
        let mut state = Self {
            items,
            query: String::new(),
            preedit: String::new(),
            filtered_items: Vec::new(),
            highlights: Vec::new(),
            selected_idx: 0,
            scroll_offset: 0,
            visible_lines: config.lines,
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
        let results = matcher::filter(&items, &self.query);

        self.filtered_items = results.iter().map(|r| r.original_idx).collect();
        self.highlights = results.iter().map(|r| r.highlight_indices.clone()).collect();

        if self.selected_idx >= self.filtered_items.len() {
            self.selected_idx = 0;
        }
        // 过滤后检查是否需要调整滚动
        self.adjust_scroll();
        self.need_redraw = true;
    }

    // 以下为未来将接入的事件接口
    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
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

    // 新增：自动调整滚动偏移逻辑
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

    pub fn select_current(&mut self) {
        if let Some(&idx) = self.filtered_items.get(self.selected_idx) {
            self.output = Some(self.items[idx].clone());
            self.selected_original_idx = Some(idx); // 记录原始索引
        }
        self.exit_code = Some(0);
    }

    pub fn cancel(&mut self) {
        self.exit_code = Some(1);
    }
}
