// src/render.rs

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent};
use crate::config::Config;
use crate::state::State;

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

        // 1. 高效填充全局背景
        let bg_pixel = [bg_b, bg_g, bg_r, 0xFF];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&bg_pixel);
        }

        // 2. 构造文本
        let mut full_text = String::new();
        full_text.push_str(&format!("{}{}{}\n", config.prompt, state.query, state.preedit));
        for (i, &idx) in state.filtered_items.iter().take(config.lines as usize).enumerate() {
            if i == state.selected_idx {
                full_text.push_str(&format!("> {}\n", state.items[idx]));
            } else {
                full_text.push_str(&format!("  {}\n", state.items[idx]));
            }
        }

        // 3. 统一排版
        self.buffer.set_metrics(&mut self.font_system, metrics);
        self.buffer.set_size(&mut self.font_system, width as f32, height as f32);

        // 显式指定 Noto Sans CJK SC，如果系统找不到会自动回退到 SansSerif
        let attrs = Attrs::new()
            .family(Family::Name("Noto Sans CJK SC"))
            .family(Family::SansSerif);

        self.buffer.set_text(&mut self.font_system, &full_text, attrs, Shaping::Advanced);
        self.buffer.shape_until_scroll(&mut self.font_system, false);

        let runs: Vec<_> = self.buffer.layout_runs().collect();
        let num_runs = runs.len();

        // 4. 画背景色 (完全绑定 cosmic_text 的真实行坐标)
        for (i, run) in runs.iter().enumerate() {
            let y_top = run.line_top.floor() as i32;
            // 修复：背景高度严格使用固定行高，绝不延伸到画布底部
            let rect_h = line_h as i32;

            if i == 0 {
                fill_rect(pixels, stride, width, height, 0, y_top, width, rect_h, pbg_r, pbg_g, pbg_b);
            } else if i - 1 == state.selected_idx {
                fill_rect(pixels, stride, width, height, 0, y_top, width, rect_h, sbg_r, sbg_g, sbg_b);
            }
        }

        // 5. 画文字
        for (i, run) in runs.iter().enumerate() {
            let (r, g, b) = if i == 0 {
                (pfg_r, pfg_g, pfg_b)
            } else if i - 1 == state.selected_idx {
                (sfg_r, sfg_g, sfg_b)
            } else {
                (fg_r, fg_g, fg_b)
            };

            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                if let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical.cache_key) {
                    let placement = image.placement;

                    // 致命修复：必须应用 placement 偏移，将原点坐标转换为位图左上角坐标
                    let px = physical.x + placement.left;
                    let py = physical.y - placement.top; // Y 轴是向上的，所以是减

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
