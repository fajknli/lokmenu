// src/render.rs

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Wrap};
use std::collections::HashSet;
use crate::config::Config;
use crate::state::State;

struct LineInfo {
    text: String,
    is_selected: bool,
    is_marked: bool,
    highlights: HashSet<usize>,
    prefix_byte_len: usize,
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
    pub fn new(font_name: &str) -> Self {
        let mut db = cosmic_text::fontdb::Database::new();

        // 1. 优先加载用户指定的字体
        if !font_name.is_empty() {
            if let Some(path) = fc_match(font_name) {
                let _ = db.load_font_file(&path);
            } else {
                eprintln!("Warning: Font '{}' not found via fc-match, falling back.", font_name);
            }
        }

        // 2. 加载一个保底的中文字体，防止中文乱码
        for pattern in &["sans:lang=zh", "sans-serif", "sans"] {
            if let Some(path) = fc_match(pattern) {
                if db.load_font_file(&path).is_ok() {
                    break;
                }
            }
        }

        if db.faces().count() == 0 {
            eprintln!("lokmenu: error: no fonts found. Please install fonts.");
            std::process::exit(1);
        }

        let mut font_system = FontSystem::new_with_locale_and_db("en".to_string(), db);

        let metrics = Metrics::new(18.0, 27.0);
        let buffer = Buffer::new(&mut font_system, metrics);
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            buffer,
            font_checked: true,
            last_text: String::new(),
            last_width: 0,
            last_height: 0,
            last_scale: 1.0,
            cached_line_h: 0.0,
            cached_font_size: 0.0,
            cached_font_name: String::new(),
        }
    }

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

    pub fn draw_frame(&mut self, pixels: &mut [u8], width: i32, height: i32, scale: f32, state: &State, config: &Config) -> Option<(i32, i32)> {
        let stride = width * 4;
        let font_size = config.font_size * scale;

        // 测量实际行高
        let base_line_h = self.measure_line_height(font_size, &config.font);
        let line_h = base_line_h * 1.15;
        let metrics = Metrics::new(font_size, line_h);

        let (bg_r, bg_g, bg_b, bg_a) = extract_rgba(config.bg);
        let (fg_r, fg_g, fg_b, _) = extract_rgba(config.fg);
        let (sbg_r, sbg_g, sbg_b, sbg_a) = extract_rgba(config.sbg);
        let (sfg_r, sfg_g, sfg_b, _) = extract_rgba(config.sfg);
        let (hfg_r, hfg_g, hfg_b, _) = extract_rgba(config.hfg);
        let (pfg_r, pfg_g, pfg_b, _) = extract_rgba(config.prompt_fg);
        let (pbg_r, pbg_g, pbg_b, pbg_a) = extract_rgba(config.prompt_bg);
        let (pxfg_r, pxfg_g, pxfg_b, _) = extract_rgba(config.prefix_fg);
        let (pxbg_r, pxbg_g, pxbg_b, pxbg_a) = extract_rgba(config.prefix_bg);

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

        let prompt_prefix_len = config.prompt.len();
        let prompt_line = LineInfo {
            text: format!("{}{}{}", config.prompt, display_query, state.preedit),
            is_selected: false,
            is_marked: false,
            highlights: HashSet::new(),
            prefix_byte_len: prompt_prefix_len,
        };

        let mut list_lines: Vec<LineInfo> = Vec::new();

        // 非密码模式才渲染列表
        if !config.password {
            if state.filtered_items.is_empty() {
                list_lines.push(LineInfo {
                    text: state.query.clone(),
                    is_selected: true,
                    is_marked: false,
                    highlights: HashSet::new(),
                    prefix_byte_len: 0,
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

                    list_lines.push(LineInfo {
                        text: display_item,
                        is_selected,
                        is_marked,
                        highlights,
                        prefix_byte_len: 0,
                    });
                }
            }
        }

        let is_bottom = config.anchor.is_bottom();
        let prompt_idx: usize;

        if is_bottom {
            let padding = config.lines.saturating_sub(list_lines.len() as u32) as usize;
            for _ in 0..padding {
                lines.push(LineInfo {
                    text: String::new(),
                    is_selected: false,
                    is_marked: false,
                    highlights: HashSet::new(),
                    prefix_byte_len: 0,
                });
            }
            list_lines.reverse();
            lines.extend(list_lines);
            prompt_idx = lines.len();
            lines.push(prompt_line);
        } else {
            prompt_idx = 0;
            lines.push(prompt_line);
            lines.extend(list_lines);
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

            // 始终包含通用无衬线字体作为保底
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
        // 抗锯齿对齐优化基于实际渲染矩形高度进行精确居中，消除小数截断导致的发虚
        let text_y_offset = ((actual_rect_h as f32 - base_line_h) / 2.0).round() as i32;


        for (i, run) in runs.iter().enumerate() {
            if i >= lines.len() { break; }
            let y_top = run.line_top.floor() as i32;

            if i == prompt_idx {
                let prompt_info = &lines[i];
                let left_padding = (8.0 * scale).round() as i32;
                if prompt_info.prefix_byte_len > 0 {
                    let prefix_end_x = estimate_prefix_pixel_width(
                        prompt_info.prefix_byte_len, left_padding, run, scale,
                    );
                    fill_rect(pixels, stride, width, height, 0, y_top, prefix_end_x, actual_rect_h, pxbg_r, pxbg_g, pxbg_b, pxbg_a);
                    // 输入段背景
                    fill_rect(pixels, stride, width, height, prefix_end_x, y_top, width - prefix_end_x, actual_rect_h, pbg_r, pbg_g, pbg_b, pbg_a);
                } else {
                    // 无前缀，整行用 prompt_bg
                    fill_rect(pixels, stride, width, height, 0, y_top, width, actual_rect_h, pbg_r, pbg_g, pbg_b, pbg_a);
                }
            } else {
                // 列表行背景（和原来一样）
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
                    let py = physical.y - placement.top + text_y_offset;

                    let char_idx = line_info.text.get(..glyph.start)
                        .map(|s| s.chars().count())
                        .unwrap_or(0);

                    let is_highlight = line_info.highlights.contains(&char_idx);

                    let (r, g, b) = if line_info.is_selected && is_highlight {
                        (hfg_r, hfg_g, hfg_b)  // 选中 + 匹配字符，用高亮色
                    } else if line_info.is_selected {
                        (sfg_r, sfg_g, sfg_b)  // 选中但非匹配字符，用选中色
                    } else if i == prompt_idx {
                        if line_info.prefix_byte_len > 0 && (glyph.start as i32) < line_info.prefix_byte_len as i32 {
                            (pxfg_r, pxfg_g, pxfg_b)
                        } else {
                            (pfg_r, pfg_g, pfg_b)
                        }
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
        // 在 prompt 行基于 cursor_pos 画竖线
        if state.preedit.is_empty() {
            if let Some(prompt_run) = runs.get(prompt_idx) {
                let left_padding = (8.0 * scale).round() as i32;
                let y_top = prompt_run.line_top.floor() as i32;
                let cursor_w = (2.0 * scale).round() as i32;

                let cursor_h = (base_line_h * 0.95).round() as i32;
                let cursor_y = y_top + ((actual_rect_h - cursor_h) / 2).max(0);

                let (cr, cg, cb) = (pfg_r, pfg_g, pfg_b);

                // 计算光标对应的字节偏移量
                // display_query 是实际显示的内容，需要考虑密码模式的星号
                let cursor_char_pos = state.cursor_pos.min(display_query.chars().count());
                let cursor_byte_offset = display_query.char_indices()
                    .nth(cursor_char_pos)
                    .map(|(b, _)| b)
                    .unwrap_or_else(|| display_query.len());

                // 加上 prompt 的字节长度，因为在渲染时 prompt 和 query 是拼在一起的
                let target_byte_pos = prompt_prefix_len + cursor_byte_offset;

                // 在 glyphs 中找到光标应在的 x 坐标
                let mut cursor_x = left_padding; // 默认在行首
                for glyph in prompt_run.glyphs.iter() {
                    let phys = glyph.physical((0.0, prompt_run.line_y), scale);
                    let glyph_end_x = phys.x + (glyph.w * scale).round() as i32 + left_padding;

                    // 如果某个字形的结尾小于等于光标位置，光标就在它后面
                    if glyph.end <= target_byte_pos {
                        cursor_x = glyph_end_x;
                    } else {
                        // 因为 glyphs 是顺序排的，一旦超过了光标位置就可以跳出
                        break;
                    }
                }

                // 如果列表为空，或者没有命中任何 glyph，确保光标在 padding 处
                if prompt_run.glyphs.is_empty() {
                    cursor_x = left_padding;
                }

                fill_rect(pixels, stride, width, height, cursor_x, cursor_y, cursor_w, cursor_h, cr, cg, cb, 255);

                // 返回光标的物理坐标
                return Some((cursor_x, cursor_y));
            }
        }

        None // 没有画光标，返回 None
    }
}

fn fc_match(pattern: &str) -> Option<String> {
    std::process::Command::new("fc-match")
        .args(["--format", "%{file}", pattern])
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        })
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
    let x_end = x.saturating_add(w).min(buf_w);
    let y_start = y.max(0);
    let y_end = y.saturating_add(h).min(buf_h);

    if x_start >= x_end || y_start >= y_end { return; }

    let row_len = (x_end - x_start) as usize * 4;
    let start_x_byte = x_start * 4;
    let color_pixel = [b, g, r, a];

    // 使用 chunks_exact_mut 直接按像素填充，零分配
    for py in y_start..y_end {
        let start_idx = (py * stride + start_x_byte) as usize;
        let row_slice = &mut pixels[start_idx..start_idx + row_len];
        for chunk in row_slice.chunks_exact_mut(4) {
            chunk.copy_from_slice(&color_pixel);
        }
    }
}

fn estimate_prefix_pixel_width(
    prefix_byte_len: usize,
    left_padding: i32,
    run: &cosmic_text::LayoutRun,
    scale: f32,
) -> i32 {
    for glyph in run.glyphs.iter() {
        if glyph.start >= prefix_byte_len {
            let phys = glyph.physical((0.0, run.line_y), scale);
            return phys.x + left_padding;
        }
    }
    if let Some(last) = run.glyphs.last() {
        let phys = last.physical((0.0, run.line_y), scale);
        phys.x + (last.w * scale).round() as i32 + left_padding
    } else {
        left_padding
    }
}
