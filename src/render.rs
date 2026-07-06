// src/render.rs

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Wrap};
use crate::config::Config;
use crate::state::State;

struct LineInfo {
    text: String,
    is_selected: bool,
    is_marked: bool,
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
    last_scale: f32,
    cached_line_h: f32,
    cached_font_size: f32,
    cached_font_name: String,
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
            last_scale: 1.0,
            cached_line_h: 0.0,
            cached_font_size: 0.0,
            cached_font_name: String::new(),
        }
    }

    /// 测量字体的实际行高（考虑中英文混合）
    /// 通过构造两行文本，计算 line_top 的差值来获得最精确的实际行高
    pub fn measure_line_height(&mut self, font_size: f32, font: &str) -> f32 {
        if self.cached_line_h > 0.0
            && (self.cached_font_size - font_size).abs() < 0.01
            && self.cached_font_name == font
        {
            return self.cached_line_h;
        }

        let metrics = Metrics::new(font_size, font_size * 1.5);
        let mut buf = Buffer::new(&mut self.font_system, metrics);

        // 设定足够大的尺寸防止换行
        buf.set_size(&mut self.font_system, 10000.0, 10000.0);

        let attrs = Attrs::new()
            .family(Family::Name(font))
            .family(Family::SansSerif);

        // 放入两行包含中英文的文字
        buf.set_text(&mut self.font_system, "Ayg中文\nAyg中文", attrs, Shaping::Advanced);
        buf.shape_until_scroll(&mut self.font_system, false);

        let mut runs = buf.layout_runs();
        let mut line_h = font_size * 1.5; // 默认 fallback

        // 获取第一行和第二行的 line_top，它们的差值就是完美的行高
        if let Some(first) = runs.next() {
            if let Some(second) = runs.next() {
                line_h = second.line_top - first.line_top;
            }
        }

        self.cached_line_h = line_h;
        self.cached_font_size = font_size;
        self.cached_font_name = font.to_string();

        line_h
    }

    pub fn draw_frame(&mut self, pixels: &mut [u8], width: i32, height: i32, scale: f32, state: &State, config: &Config) {
        let stride = width * 4;
        let font_size = config.font_size * scale;

        // 测量实际行高
        let base_line_h = self.measure_line_height(font_size, &config.font);
        let line_h = base_line_h + 6.0 * scale; // 增加 6 逻辑像素的行间距
        let metrics = Metrics::new(font_size, line_h);

        let (bg_r, bg_g, bg_b, bg_a) = extract_rgba(config.bg);
        let (fg_r, fg_g, fg_b, _) = extract_rgba(config.fg);
        let (sbg_r, sbg_g, sbg_b, sbg_a) = extract_rgba(config.sbg);
        let (sfg_r, sfg_g, sfg_b, _) = extract_rgba(config.sfg);
        let (hfg_r, hfg_g, hfg_b, _) = extract_rgba(config.hfg);
        let (pbg_r, pbg_g, pbg_b, pbg_a) = extract_rgba(config.prompt_bg);
        let (pfg_r, pfg_g, pfg_b, _) = extract_rgba(config.prompt_fg);

        let bg_pixel = [bg_b, bg_g, bg_r, bg_a]; // 使用带透明度的像素
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&bg_pixel);
        }

        let mut lines: Vec<LineInfo> = Vec::new();

        // 密码模式下，用星号代替输入内容
        let display_query = if config.password {
            "*".repeat(state.query.chars().count())
        } else {
            state.query.clone()
        };

        lines.push(LineInfo {
            text: format!("{}{}{}", config.prompt, display_query, state.preedit),
            is_selected: false,
            is_marked: false,
            highlights: Vec::new(),
        });

        // 非密码模式才渲染列表
        if !config.password {
            if state.filtered_items.is_empty() {
                // 没有匹配项时，直接显示输入框内容
                lines.push(LineInfo {
                    text: state.query.clone(),
                    is_selected: true,
                    is_marked: false,
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
                    let is_marked = state.marked_items.contains(&orig_idx);
                    let highlights = state.highlights.get(i).cloned().unwrap_or_default();

                    lines.push(LineInfo {
                        text: display_item,
                        is_selected,
                        is_marked,
                        highlights,
                    });
                }
            }
        }

        // 如果没有在输入中文预编辑，在第一行文本末尾追加光标
        let cursor = "│";
        if state.preedit.is_empty() {
            if let Some(first_line) = lines.get_mut(0) {
                first_line.text.push_str(cursor);
            }
        }

        let full_text = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");

        let needs_shape = full_text != self.last_text
            || width != self.last_width
            || height != self.last_height
            || scale != self.last_scale;

        if needs_shape {
            self.buffer.set_metrics(&mut self.font_system, metrics);
            self.buffer.set_wrap(&mut self.font_system, Wrap::None);
            self.buffer.set_size(&mut self.font_system, width as f32, height as f32);

            // 基础属性：始终包含通用无衬线字体作为保底
            let mut attrs = Attrs::new().family(Family::SansSerif);

            // 如果用户指定了具体字体名称，则添加到最前面优先匹配
            if !config.font.is_empty() {
                if !self.font_checked {
                    let db = self.font_system.db();
                    let found = db.faces().any(|f| f.families.iter().any(|(name, _)| name == &config.font));
                    if !found {
                        eprintln!("Warning: Font '{}' not found, falling back to system default.", config.font);
                    }
                    self.font_checked = true;
                }
                attrs = attrs.family(Family::Name(&config.font));
            }

            self.buffer.set_text(&mut self.font_system, &full_text, attrs, Shaping::Advanced);
            self.buffer.shape_until_scroll(&mut self.font_system, false);

            self.last_text = full_text;
            self.last_width = width;
            self.last_height = height;
            self.last_scale = scale;
        }

        let runs: Vec<_> = self.buffer.layout_runs().collect();

        let actual_rect_h = line_h.ceil() as i32;

        for (i, run) in runs.iter().enumerate() {
            if i >= lines.len() { break; }
            let y_top = run.line_top.floor() as i32;

            if i == 0 {
                fill_rect(pixels, stride, width, height, 0, y_top, width, actual_rect_h, pbg_r, pbg_g, pbg_b, pbg_a);
            } else {
                if lines[i].is_selected {
                    fill_rect(pixels, stride, width, height, 0, y_top, width, actual_rect_h, sbg_r, sbg_g, sbg_b, sbg_a);
                }

                if lines[i].is_marked {
                    let line_width = (3.0 * scale).round() as i32;
                    fill_rect(pixels, stride, width, height, 0, y_top, line_width, actual_rect_h, hfg_r, hfg_g, hfg_b, 255);
                } else if lines[i].is_selected {
                    let line_width = (3.0 * scale).round() as i32;
                    fill_rect(pixels, stride, width, height, 0, y_top, line_width, actual_rect_h, sfg_r, sfg_g, sfg_b, 255);
                }
            }
        }

        for (i, run) in runs.iter().enumerate() {
            if i >= lines.len() { break; }
            let line_info = &lines[i];

            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, run.line_y), scale);
                if let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical.cache_key) {
                    let placement = image.placement;
                    let left_padding = (8.0 * scale).round() as i32;
                    let px = physical.x + placement.left + left_padding;
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
                        (hfg_r, hfg_g, hfg_b)
                    } else {
                        (fg_r, fg_g, fg_b)
                    };

                    match image.content {
                        SwashContent::Mask => {
                            for yy in 0..placement.height as i32 {
                                for xx in 0..placement.width as i32 {
                                    let target_x = px + xx;
                                    let target_y = py + yy;
                                    if target_x >= 0 && target_x < width && target_y >= 0 && target_y < height {
                                        let buf_idx = (target_y * stride + target_x * 4) as usize;
                                        let img_idx = yy as usize * placement.width as usize + xx as usize;
                                        if img_idx < image.data.len() {
                                            let alpha = image.data[img_idx];
                                            if alpha > 0 {
                                                blend_pixel_fast(pixels, buf_idx, r, g, b, alpha);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        SwashContent::Color => {
                            for yy in 0..placement.height as i32 {
                                for xx in 0..placement.width as i32 {
                                    let target_x = px + xx;
                                    let target_y = py + yy;
                                    if target_x >= 0 && target_x < width && target_y >= 0 && target_y < height {
                                        let buf_idx = (target_y * stride + target_x * 4) as usize;
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
                                }
                            }
                        }
                        SwashContent::SubpixelMask => {
                            for yy in 0..placement.height as i32 {
                                for xx in 0..placement.width as i32 {
                                    let target_x = px + xx;
                                    let target_y = py + yy;
                                    if target_x >= 0 && target_x < width && target_y >= 0 && target_y < height {
                                        let buf_idx = (target_y * stride + target_x * 4) as usize;
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

fn extract_rgba(c: u32) -> (u8, u8, u8, u8) {
    (((c >> 16) & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, (c & 0xFF) as u8, ((c >> 24) & 0xFF) as u8)
}

fn blend_pixel_fast(pixels: &mut [u8], idx: usize, r: u8, g: u8, b: u8, alpha: u8) {
    let a = alpha as u32;
    let inv_alpha = 255 - a;
    let bg_b = pixels[idx] as u32;
    let bg_g = pixels[idx + 1] as u32;
    let bg_r = pixels[idx + 2] as u32;
    let bg_a = pixels[idx + 3] as u32;

    let out_a = a + (bg_a * inv_alpha) / 255;
    if out_a > 0 {
        pixels[idx]     = ((bg_b * inv_alpha + b as u32 * a) / out_a) as u8;
        pixels[idx + 1] = ((bg_g * inv_alpha + g as u32 * a) / out_a) as u8;
        pixels[idx + 2] = ((bg_r * inv_alpha + r as u32 * a) / out_a) as u8;
        pixels[idx + 3] = out_a as u8;
    } else {
        pixels[idx] = 0;
        pixels[idx + 1] = 0;
        pixels[idx + 2] = 0;
        pixels[idx + 3] = 0;
    }
}

fn fill_rect(pixels: &mut [u8], stride: i32, buf_w: i32, buf_h: i32, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8, a: u8) {
    let x_start = x.max(0);
    let x_end = (x + w).min(buf_w);
    let y_start = y.max(0);
    let y_end = (y + h).min(buf_h);

    if x_start >= x_end || y_start >= y_end { return; }

    let row_len = (x_end - x_start) as usize * 4;
    let color_pixel = [b, g, r, a];

    // 构造一整行像素数据
    let row: Vec<u8> = color_pixel.iter().copied().cycle().take(row_len).collect();

    for py in y_start..y_end {
        let start_idx = (py * stride + x_start * 4) as usize;
        // 按行一次性拷贝
        pixels[start_idx..start_idx + row_len].copy_from_slice(&row);
    }
}
