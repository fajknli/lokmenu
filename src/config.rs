use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum WindowAnchor {
    Top,
    Bottom,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub prompt: String,
    pub output_index: bool,
    pub lines: u32,
    pub width: u32,
    pub font_size: f32,
    pub font: String,
    pub bg: u32,
    pub fg: u32,
    pub sbg: u32,
    pub sfg: u32,
    pub hfg: u32,
    pub prompt_bg: u32,
    pub prompt_fg: u32,
    pub multi_select: bool,
    pub null: bool,
    pub password: bool,
    pub anchor: WindowAnchor, // 改为 WindowAnchor 枚举
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            output_index: false,
            lines: 8,
            width: 800,
            font_size: 14.0,
            font: String::new(),
            bg: 0xFF141522,
            fg: 0xFFA9B5D5,
            sbg: 0xFF565D7E,
            sfg: 0xFFD4DCF2,
            hfg: 0xFFC93B3B,
            prompt_bg: 0xFF1B1D2B,
            prompt_fg: 0xFFA9B5D5,
            multi_select: false,
            null: false,
            password: false,
            anchor: WindowAnchor::Top, // 默认值
        }
    }
}
