// src/config.rs

#[derive(Debug, Clone)]
pub struct Config {
    pub prompt: String,
    pub output_index: bool,
    pub lines: u32,
    pub width: u32,
    pub font_size: f32,
    pub bg: u32,
    pub fg: u32,
    pub sbg: u32,
    pub sfg: u32,
    pub prompt_bg: u32,
    pub prompt_fg: u32, // 新增
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            output_index: false,
            lines: 8,
            width: 800,
            font_size: 18.0,
            bg: 0xFF111111,
            fg: 0xFFCCCCCC,
            sbg: 0xFF333333,
            sfg: 0xFFFFFFFF,
            prompt_bg: 0xFF1E90FF,
            prompt_fg: 0xFFFFFFFF, // 默认白
        }
    }
}
