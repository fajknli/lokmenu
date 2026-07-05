// src/render.rs

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent};
use crate::config::Config;
use crate::state::State;

// 辅助结构体：记录每一行的渲染信息
struct LineInfo {
    text: String,
    is_selected: bool,
    highlights: Vec<usize>, // 相对于该行 text 的字符索引
}

pub struct Renderer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub buffer: Buffer,
}

impl Renderer {
    pub fn new() -> Self {
        let mut font_system = FontSystem::new();
        let metrics = Metrics::new(18.0, 27.0);
        let buffer = Buffer::new(&mut font_system, metrics);
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            buffer,
        }
    }

    pub fn draw_frame(&mut self, pixels: &mut [u8], width: i32, height: i32, state: &State, config: &Config) {
        let stride = width * 4;
        let font_size = config.font_size;
        let line_h = (font_size * 1.5).ceil();
        let metrics = Metrics::new(font_size, line_h);

        let extract_rgb = |c: u32| -> (u8, u8, u8) {
            ((c >> 16 & 0xFF) as u8, (c >> 8 & 0xFF) as u8, (c & 0xFF) as u8)
        };

        let (bg_r, bg_g, bg_b) = extract_rgb(config.bg);
        let (fg_r, fg_g, fg_b) = extract_rgb(config.fg);
        let (sbg_r, sbg_g, sbg_b) = extract_rgb(config.sbg);
        let (sfg_r, sfg_g, sfg_b) = extract_rgb(config.sfg);
        let (pbg_r, pbg_g, pbg_b) = extract_rgb(config.prompt_bg);
        let (pfg_r, pfg_g, pfg_b) = extract_rgb(config.prompt_fg);

        // 1. 填充全局背景
        let bg_pixel = [bg_b, bg_g, bg_r, 0xFF];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&bg_pixel);
        }

        // 2. 构造每一行的文本及属性
        let mut lines: Vec<LineInfo> = Vec::new();

        // 第一行：Prompt
        lines.push(LineInfo {
            text: format!("{}{}{}", config.prompt, state.query, state.preedit),
            is_selected: false,
            highlights: Vec::new(),
        });

        // 列表行：支持滚动与空结果提示
        if state.filtered_items.is_empty() {
            lines.push(LineInfo {
                text: "  No matches found".to_string(),
                is_selected: false,
                highlights: Vec::new(),
            });
        } else {
            let max_chars = 80; // 截断阈值
            let visible_end = state.scroll_offset + config.lines as usize;

            for i in state.scroll_offset..visible_end.min(state.filtered_items.len()) {
                let orig_idx = state.filtered_items[i];
                let item_str = &state.items[orig_idx];

                // 文字过长截断
                let display_item: String = if item_str.chars().count() > max_chars {
                    let mut s: String = item_str.chars().take(max_chars).collect();
                    s.push_str("...");
                    s
                } else {
                    item_str.clone()
                };

                let is_selected = i == state.selected_idx;
                let prefix = if is_selected { "> " } else { "  " };
                let prefix_chars = prefix.chars().count();

                // 转换高亮索引：加上前缀的字符数偏移
                let highlights: Vec<usize> = state.highlights[i].iter()
                    .map(|&h| h + prefix_chars)
                    .filter(|&h| h < display_item.chars().count() + prefix_chars)
                    .collect();

                lines.push(LineInfo {
                    text: format!("{}{}", prefix, display_item),
                    is_selected,
                    highlights,
                });
            }
        }

        // 拼接全文本
        let full_text = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");

        // 3. cosmic-text 统一排版
        self.buffer.set_metrics(&mut self.font_system, metrics);
        self.buffer.set_size(&mut self.font_system, width as f32, height as f32);

        let attrs = Attrs::new()
            .family(Family::Name("Noto Sans CJK SC"))
            .family(Family::SansSerif);

        self.buffer.set_text(&mut self.font_system, &full_text, attrs, Shaping::Advanced);
        self.buffer.shape_until_scroll(&mut self.font_system, false);

        let runs: Vec<_> = self.buffer.layout_runs().collect();

        // 4. 画背景色
        for (i, run) in runs.iter().enumerate() {
            if i >= lines.len() { break; }
            let y_top = run.line_top.floor() as i32;
            let rect_h = line_h as i32;

            if i == 0 {
                fill_rect(pixels, stride, width, height, 0, y_top, width, rect_h, pbg_r, pbg_g, pbg_b);
            } else if lines[i].is_selected {
                fill_rect(pixels, stride, width, height, 0, y_top, width, rect_h, sbg_r, sbg_g, sbg_b);
            }
        }

        // 5. 画文字 (包含高亮逻辑)
        for (i, run) in runs.iter().enumerate() {
            if i >= lines.len() { break; }
            let line_info = &lines[i];

            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                if let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical.cache_key) {
                    let placement = image.placement;

                    let px = physical.x + placement.left;
                    let py = physical.y - placement.top;

                    // 计算当前字形在行文本中的字符索引
                    let char_idx = line_info.text[..glyph.start].chars().count();
                    let is_highlight = line_info.highlights.contains(&char_idx);

                    // 确定当前字形的颜色
                    let (r, g, b) = if line_info.is_selected {
                        (sfg_r, sfg_g, sfg_b) // 选中行：使用选中前景色
                    } else if i == 0 {
                        (pfg_r, pfg_g, pfg_b) // 输入行：使用输入前景色
                    } else if is_highlight {
                        (sfg_r, sfg_g, sfg_b) // 普通行高亮：使用选中前景色(通常更亮)以示强调
                    } else {
                        (fg_r, fg_g, fg_b)    // 普通文字
                    };

                    for yy in 0..placement.height as i32 {
                        for xx in 0..placement.width as i32 {
                            let target_x = px + xx;
                            let target_y = py + yy;

                            if target_x >= 0 && target_x < width && target_y >= 0 && target_y < height {
                                let buf_idx = (target_y * stride + target_x * 4) as usize;

                                match image.content {
                                    SwashContent::Mask => {
                                        let img_idx = yy as usize * placement.width as usize + xx as usize;
                                        if img_idx < image.data.len() {
                                            let alpha = image.data[img_idx] as f32 / 255.0;
                                            if alpha > 0.0 {
                                                blend_pixel(pixels, buf_idx, r, g, b, alpha);
                                            }
                                        }
                                    }
                                    SwashContent::Color => {
                                        let img_idx = (yy as usize * placement.width as usize + xx as usize) * 4;
                                        if img_idx + 3 < image.data.len() {
                                            let a = image.data[img_idx + 3] as f32 / 255.0;
                                            if a > 0.0 {
                                                blend_pixel(pixels, buf_idx, image.data[img_idx], image.data[img_idx + 1], image.data[img_idx + 2], a);
                                            }
                                        }
                                    }
                                    SwashContent::SubpixelMask => {
                                        let img_idx = (yy as usize * placement.width as usize + xx as usize) * 3;
                                        if img_idx + 2 < image.data.len() {
                                            let a = (image.data[img_idx].max(image.data[img_idx + 1]).max(image.data[img_idx + 2])) as f32 / 255.0;
                                            if a > 0.0 {
                                                blend_pixel(pixels, buf_idx, r, g, b, a);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn blend_pixel(pixels: &mut [u8], idx: usize, r: u8, g: u8, b: u8, alpha: f32) {
    let bg_r = pixels[idx + 2] as f32;
    let bg_g = pixels[idx + 1] as f32;
    let bg_b = pixels[idx] as f32;
    pixels[idx + 2] = (bg_r * (1.0 - alpha) + r as f32 * alpha) as u8;
    pixels[idx + 1] = (bg_g * (1.0 - alpha) + g as f32 * alpha) as u8;
    pixels[idx]     = (bg_b * (1.0 - alpha) + b as f32 * alpha) as u8;
    pixels[idx + 3] = 0xFF;
}

fn fill_rect(pixels: &mut [u8], stride: i32, buf_w: i32, buf_h: i32, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8) {
    let x_start = x.max(0);
    let x_end = (x + w).min(buf_w);
    let y_start = y.max(0);
    let y_end = (y + h).min(buf_h);

    if x_start >= x_end || y_start >= y_end { return; }

    let row_len = (x_end - x_start) as usize * 4;
    let color_pixel = [b, g, r, 0xFF];

    for py in y_start..y_end {
        let start_idx = (py * stride + x_start * 4) as usize;
        for chunk in pixels[start_idx..start_idx + row_len].chunks_exact_mut(4) {
            chunk.copy_from_slice(&color_pixel);
        }
    }
}
