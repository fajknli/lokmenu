use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum WindowAnchor {
    Top,
    TopLeft,
    TopCenter,
    TopRight,
    Bottom,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl WindowAnchor {
    pub fn is_bottom(&self) -> bool {
        matches!(self, Self::Bottom | Self::BottomLeft | Self::BottomCenter | Self::BottomRight)
    }

    pub fn is_full_width(&self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
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
    pub prompt_fg: u32,
    pub prompt_bg: u32,
    pub prefix_fg: u32,
    pub prefix_bg: u32,
    pub multi_select: bool,
    pub null: bool,
    pub password: bool,
    pub anchor: WindowAnchor,
    pub no_history: bool,
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
            bg:        0xFF141522,
            fg:        0xFFD4DCF2,
            sbg:       0xFF242838,
            sfg:       0xFFD4DCF2,
            hfg:       0xFFD70000,
            prompt_bg: 0xFF1B1D2B,
            prompt_fg: 0xFFD4DCF2,
            prefix_fg: 0xFFD4DCF2,
            prefix_bg: 0xFF1B1D2B,
            multi_select: false,
            null: false,
            password: false,
            anchor: WindowAnchor::Top,
            no_history: false,
        }
    }
}
