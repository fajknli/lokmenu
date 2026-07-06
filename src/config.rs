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
    pub hfg: u32,        // 新增：高亮匹配字符颜色
    pub prompt_bg: u32,
    pub prompt_fg: u32,
    pub multi_select: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            output_index: false,
            lines: 8,
            width: 800,
            font_size: 14.0,
            font: "Noto Sans".to_string(),
            bg: 0xFF141522,       // 极暗冷黑
            fg: 0xFFA9B5D5,       // 基准冷白
            sbg: 0xFF565D7E,      // 深冷灰 (提亮选中背景)
            sfg: 0xFFD4DCF2,      // 亮冷白 (选中项文字)
            hfg: 0xFFC93B3B,      // 冷警示红 (匹配字符高亮)
            prompt_bg: 0xFF1B1D2B,// 微冷黑 (压暗输入框背景)
            prompt_fg: 0xFFA9B5D5,// 基准冷白
            multi_select: false,
        }
    }
}
