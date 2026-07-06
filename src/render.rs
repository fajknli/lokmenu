// src/render.rs

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Wrap};
use crate::config::Config;
use crate::state::State;

struct LineInfo {
    text: String,
    is_selected: bool,
    highlights: Vec<usize>,
}

pub struct Renderer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub buffer: Buffer,
    font_checked: bool,
    last_text: String,
    last_width: i32,
    last_height: i32,
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
            font_checked: false,
            last_text: String::new(),
            last_width: 0,
            last_height: 0,
        }
    }

    pub fn draw_frame(&mut self, pixels: &mut [u8], width: i32, height: i32, state: &State, config: &Config) {
        let stride = width * 4;
        let font_size = config.font_size;
        let line_h = (font_size * 1.5).ceil();
        let metrics = Metrics::new(font_size, line_h);

        let (bg_r, bg_g, bg_b) = extract_rgb(config.bg);
        let (fg_r, fg_g, fg_b) = extract_rgb(config.fg);
        let (sbg_r, sbg_g, sbg_b) = extract_rgb(config.sbg);
        let (sfg_r, sfg_g, sfg_b) = extract_rgb(config.sfg);
        let (hfg_r, hfg_g, hfg_b) = extract_rgb(config.hfg);
        let (pbg_r, pbg_g, pbg_b) = extract_rgb(config.prompt_bg);
        let (pfg_r, pfg_g, pfg_b) = extract_rgb(config.prompt_fg);

        let bg_pixel = [bg_b, bg_g, bg_r, 0xFF];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&bg_pixel);
        }

        let mut lines: Vec<LineInfo> = Vec::new();
        lines.push(LineInfo {
            text: format!("{}{}{}", config.prompt, state.query, state.preedit),
            is_selected: false,
            highlights: Vec::new(),
        });

        if state.filtered_items.is_empty() {
            lines.push(LineInfo {
                text: "No matches found".to_string(),
                is_selected: false,
                highlights: Vec::new(),
            });
        } else {
            let max_chars = ((width as f32 / (font_size * 0.65)) as usize).max(10);
            let visible_end = state.scroll_offset + config.lines as usize;

            for i in state.scroll_offset..visible_end.min(state.filtered_items.len()) {
                let orig_idx = state.filtered_items[i];
                let item_str = &state.items[orig_idx];

                let display_item: String = if item_str.chars().count() > max_chars {
                    let mut s: String = item_str.chars().take(max_chars).collect();
                    s.push_str("...");
                    s
                } else {
                    item_str.clone()
                };

                let is_selected = i == state.selected_idx;
                let highlights = state.highlights.get(i).cloned().unwrap_or_default();

                lines.push(LineInfo {
                    text: display_item,
                    is_selected,
                    highlights,
                });
            }
        }

        let full_text = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");

        // shape 缓存：文本没变且尺寸没变时跳过 shape
        let needs_shape = full_text != self.last_text
            || width != self.last_width
            || height != self.last_height;

        if needs_shape {
            self.buffer.set_metrics(&mut self.font_system, metrics);
            self.buffer.set_wrap(&mut self.font_system, Wrap::None);
            self.buffer.set_size(&mut self.font_system, width as f32, height as f32);

            let attrs = Attrs::new()
                .family(Family::Name(&config.font))
                .family(Family::SansSerif);

            if !self.font_checked {
                let db = self.font_system.db();
                let found = db.faces().any(|f| f.families.iter().any(|(name, _)| name == &config.font));
                if !found {
                    eprintln!("Warning: Font '{}' not found, falling back to SansSerif.", config.font);
                }
                self.font_checked = true;
            }

            self.buffer.set_text(&mut self.font_system, &full_text, attrs, Shaping::Advanced);
            self.buffer.shape_until_scroll(&mut self.font_system, false);

            self.last_text = full_text;
            self.last_width = width;
            self.last_height = height;
        }

        let runs: Vec<_> = self.buffer.layout_runs().collect();

        for (i, run) in runs.iter().enumerate() {
            if i >= lines.len() { break; }
            let y_top = run.line_top.floor() as i32;
            let rect_h = line_h as i32;

            if i == 0 {
                fill_rect(pixels, stride, width, height, 0, y_top, width, rect_h, pbg_r, pbg_g, pbg_b);
            } else if lines[i].is_selected {
                fill_rect(pixels, stride, width, height, 0, y_top, width, rect_h, sbg_r, sbg_g, sbg_b);
                // 选中指示：左侧 3px 竖线
                fill_rect(pixels, stride, width, height, 0, y_top, 3, rect_h, sfg_r, sfg_g, sfg_b);
            }
        }

        for (i, run) in runs.iter().enumerate() {
            if i >= lines.len() { break; }
            let line_info = &lines[i];

            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                if let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical.cache_key) {
                    let placement = image.placement;
                    let px = physical.x + placement.left;
                    let py = physical.y - placement.top;

                    let char_idx = line_info.text.get(..glyph.start)
                        .map(|s| s.chars().count())
                        .unwrap_or(0);
                    let is_highlight = line_info.highlights.contains(&char_idx);

                    let (r, g, b) = if line_info.is_selected {
                        (sfg_r, sfg_g, sfg_b)
                    } else if i == 0 {
                        (pfg_r, pfg_g, pfg_b)
                    } else if is_highlight {
                        (hfg_r, hfg_g, hfg_b) // 修改这里：使用冷红高亮
                    } else {
                        (fg_r, fg_g, fg_b)
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
                                            let alpha = image.data[img_idx];
                                            if alpha > 0 {
                                                blend_pixel_fast(pixels, buf_idx, r, g, b, alpha);
                                            }
                                        }
                                    }
                                    SwashContent::Color => {
                                        let img_idx = (yy as usize * placement.width as usize + xx as usize) * 4;
                                        if img_idx + 3 < image.data.len() {
                                            let a = image.data[img_idx + 3];
                                            if a > 0 {
                                                let er = image.data[img_idx];
                                                let eg = image.data[img_idx + 1];
                                                let eb = image.data[img_idx + 2];
                                                blend_pixel_fast(pixels, buf_idx, er, eg, eb, a);
                                            }
                                        }
                                    }
                                    SwashContent::SubpixelMask => {
                                        let img_idx = (yy as usize * placement.width as usize + xx as usize) * 3;
                                        if img_idx + 2 < image.data.len() {
                                            let a = image.data[img_idx].max(image.data[img_idx + 1]).max(image.data[img_idx + 2]);
                                            if a > 0 {
                                                blend_pixel_fast(pixels, buf_idx, r, g, b, a);
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

fn extract_rgb(c: u32) -> (u8, u8, u8) {
    ((c >> 16 & 0xFF) as u8, (c >> 8 & 0xFF) as u8, (c & 0xFF) as u8)
}

fn blend_pixel_fast(pixels: &mut [u8], idx: usize, r: u8, g: u8, b: u8, alpha: u8) {
    let a = alpha as u32;
    let inv_alpha = 255 - a;
    let bg_b = pixels[idx] as u32;
    let bg_g = pixels[idx + 1] as u32;
    let bg_r = pixels[idx + 2] as u32;

    pixels[idx]     = ((bg_b * inv_alpha + b as u32 * a) / 255) as u8;
    pixels[idx + 1] = ((bg_g * inv_alpha + g as u32 * a) / 255) as u8;
    pixels[idx + 2] = ((bg_r * inv_alpha + r as u32 * a) / 255) as u8;
    pixels[idx + 3] = 255;
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
