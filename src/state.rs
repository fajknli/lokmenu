// src/state.rs

use crate::config::Config;
use crate::matcher;

pub struct State {
    pub items: Vec<String>,
    pub query: String,
    pub preedit: String,
    pub filtered_items: Vec<usize>,
    pub selected_idx: usize,
    pub output: Option<String>,
    pub selected_original_idx: Option<usize>,
    pub exit_code: Option<i32>,
    pub need_redraw: bool,
}

impl State {
    pub fn new(items: Vec<String>, _config: &Config) -> Self {
        let mut state = Self {
            items,
            query: String::new(),
            preedit: String::new(),
            filtered_items: Vec::new(),
            selected_idx: 0,
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
        if self.selected_idx >= self.filtered_items.len() {
            self.selected_idx = 0;
        }
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
            self.need_redraw = true;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_idx + 1 < self.filtered_items.len() {
            self.selected_idx += 1;
            self.need_redraw = true;
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
