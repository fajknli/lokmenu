use std::ffi::CString;
use std::io;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};

use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

use wayland_client::protocol::{
    wl_buffer::WlBuffer, wl_compositor::WlCompositor, wl_keyboard::{self, WlKeyboard},
    wl_registry::WlRegistry, wl_seat::WlSeat, wl_shm::WlShm, wl_surface::WlSurface,
};

// 引入输入法协议
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{ContentHint, ContentPurpose, ZwpTextInputV3},
};

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache};

use crate::matcher;

pub struct WaylandState {
    items: Vec<String>,
}

pub struct App {
    pub state: Option<WaylandState>,
    pub layer_shell: ZwlrLayerShellV1,
    pub compositor: WlCompositor,
    pub shm: WlShm,
    pub surface: Option<WlSurface>,
    pub exit_code: Option<i32>,
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub query: String,
    pub need_redraw: bool,
    pub filtered_items: Vec<usize>,
    pub selected_idx: usize,
    pub output: Option<String>,

    // 输入法相关
    pub text_input_manager: ZwpTextInputManagerV3,
    pub text_input: Option<ZwpTextInputV3>,
    pub preedit: String,
}

pub fn run(items: Vec<String>) -> io::Result<Option<String>> {
    let conn = Connection::connect_to_env().map_err(|e| {
        io::Error::new(io::ErrorKind::ConnectionRefused, format!("Wayland connect failed: {:?}", e))
    })?;

    let (globals, mut queue) = registry_queue_init::<App>(&conn).unwrap();
    let qh = queue.handle();

    let compositor: WlCompositor = globals.bind(&qh, 1..=4, ()).unwrap();
    let layer_shell: ZwlrLayerShellV1 = globals.bind(&qh, 1..=4, ()).unwrap();
    let shm: WlShm = globals.bind(&qh, 1..=1, ()).unwrap();
    let seat: WlSeat = globals.bind(&qh, 1..=4, ()).unwrap();
    let text_input_manager: ZwpTextInputManagerV3 = globals.bind(&qh, 1..=1, ()).unwrap();

    let font_system = FontSystem::new();
    let swash_cache = SwashCache::new();

    let mut app = App {
        state: Some(WaylandState { items }),
        layer_shell,
        compositor,
        shm,
        surface: None,
        exit_code: None,
        font_system,
        swash_cache,
        query: String::new(),
        need_redraw: true,
        filtered_items: Vec::new(),
        selected_idx: 0,
        output: None,
        text_input_manager,
        text_input: None,
        preedit: String::new(),
    };

    app.update_filter();

    let surface = app.compositor.create_surface(&qh, ());
    app.surface = Some(surface.clone());

    // 创建输入法上下文
    let text_input = app.text_input_manager.get_text_input(&seat, &qh, ());
    app.text_input = Some(text_input);

    let layer_surface = app.layer_shell.get_layer_surface(
        &surface, None, Layer::Top, "lok".to_string(), &qh, (),
    );

    layer_surface.set_anchor(Anchor::Top | Anchor::Left);
    layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer_surface.set_size(800, 200);
    surface.commit();

    let _keyboard = seat.get_keyboard(&qh, ());

    loop {
        match queue.blocking_dispatch(&mut app) {
            Ok(_) => {
                if let Some(code) = app.exit_code {
                    return Ok(if code == 0 { app.output.clone() } else { None });
                }
                if app.need_redraw {
                    let w = 800;
                    let h = 200;
                    let buffer = create_shm_buffer(&mut app, &qh, w, h);
                    if let Some(surface) = &app.surface {
                        surface.attach(Some(&buffer), 0, 0);
                        surface.damage(0, 0, w as i32, h as i32);
                        surface.commit();
                    }
                    app.need_redraw = false;
                }
            }
            Err(e) => {
                eprintln!("Wayland dispatch error: {:?}", e);
                std::process::exit(3);
            }
        }
    }
}

impl App {
    fn update_filter(&mut self) {
        let items = self.state.as_ref().unwrap().items.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
        let results = matcher::filter(&items, &self.query);
        self.filtered_items = results.iter().map(|r| r.original_idx).collect();
        if self.selected_idx >= self.filtered_items.len() {
            self.selected_idx = 0;
        }
        self.need_redraw = true;
    }
}

fn fill_rect(pixels: &mut [u8], stride: i32, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8) {
    for py in y..(y+h) {
        for px in x..(x+w) {
            if px >= 0 && px < w && py >= 0 && py < h {
                let idx = (py * stride + px * 4) as usize;
                pixels[idx] = b; pixels[idx+1] = g; pixels[idx+2] = r; pixels[idx+3] = 0xFF;
            }
        }
    }
}

fn draw_text(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    pixels: &mut [u8],
    stride: i32,
    width: i32,
    height: i32,
    text: &str,
    y: f32,
    fg_r: u8, fg_g: u8, fg_b: u8
) {
    let line_h = 24.0;
    let metrics = Metrics::new(18.0, line_h);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_text(font_system, text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    buffer.set_size(font_system, width as f32, line_h);
    buffer.shape_until_scroll(font_system, false);

    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter() {
            let physical = glyph.physical((glyph.x, y + 4.0), 1.0);
            if let Some(image) = swash_cache.get_image(font_system, physical.cache_key) {
                let placement = image.placement;
                let px = physical.x;
                let py = physical.y;
                let is_mask = image.content == cosmic_text::SwashContent::Mask;

                for yy in 0..placement.height as i32 {
                    for xx in 0..placement.width as i32 {
                        let target_x = px + xx;
                        let target_y = py + yy;
                        if target_x >= 0 && target_x < width && target_y >= 0 && target_y < height {
                            let buf_idx = (target_y * stride + target_x * 4) as usize;
                            let img_idx = if is_mask {
                                yy as usize * placement.width as usize + xx as usize
                            } else {
                                (yy as usize * placement.width as usize + xx as usize) * 4
                            };
                            if img_idx < image.data.len() {
                                let alpha = if is_mask {
                                    image.data[img_idx] as f32 / 255.0
                                } else {
                                    if img_idx + 3 < image.data.len() { image.data[img_idx + 3] as f32 / 255.0 } else { 0.0 }
                                };
                                if alpha > 0.0 {
                                    let bg_r = pixels[buf_idx + 2] as f32;
                                    let bg_g = pixels[buf_idx + 1] as f32;
                                    let bg_b = pixels[buf_idx] as f32;
                                    pixels[buf_idx + 2] = (bg_r * (1.0 - alpha) + fg_r as f32 * alpha) as u8;
                                    pixels[buf_idx + 1] = (bg_g * (1.0 - alpha) + fg_g as f32 * alpha) as u8;
                                    pixels[buf_idx]     = (bg_b * (1.0 - alpha) + fg_b as f32 * alpha) as u8;
                                    pixels[buf_idx + 3] = 0xFF;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn create_shm_buffer(app: &mut App, qh: &QueueHandle<App>, width: u32, height: u32) -> WlBuffer {
    let w = width as i32;
    let h = height as i32;
    let stride = w * 4;
    let size = (stride * h) as usize;

    let name = CString::new("lok-shm").unwrap();
    let fd = unsafe { libc::syscall(libc::SYS_memfd_create, name.as_ptr(), 0) };
    if fd == -1 { panic!("memfd_create failed"); }
    let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };

    if unsafe { libc::ftruncate(fd.as_raw_fd(), size as i64) } == -1 {
        panic!("ftruncate failed");
    }

    let ptr = unsafe {
        libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd.as_raw_fd(), 0) as *mut u8
    };
    if ptr == libc::MAP_FAILED as *mut u8 { panic!("mmap failed"); }

    let pixels = unsafe { std::slice::from_raw_parts_mut(ptr, size) };

    fill_rect(pixels, stride, 0, 0, w, h, 0x11, 0x11, 0x11);

    let line_h = 24.0;
    let mut y_pos = 4.0;

    // 画输入框 (蓝底白字)，将 preedit 接在 query 后面显示
    fill_rect(pixels, stride, 0, 0, w, line_h as i32, 0x1E, 0x90, 0xFF);
    let display_query = format!("> {}{}", app.query, app.preedit);
    draw_text(&mut app.font_system, &mut app.swash_cache, pixels, stride, w, h, &display_query, y_pos, 0xFF, 0xFF, 0xFF);
    y_pos += line_h;

    // 画候选列表
    let items = &app.state.as_ref().unwrap().items;
    for (i, &idx) in app.filtered_items.iter().take(8).enumerate() {
        if i == app.selected_idx {
            fill_rect(pixels, stride, 0, y_pos as i32, w, line_h as i32, 0x33, 0x33, 0x33);
            draw_text(&mut app.font_system, &mut app.swash_cache, pixels, stride, w, h, &format!("> {}", items[idx]), y_pos, 0xFF, 0xFF, 0xFF);
        } else {
            draw_text(&mut app.font_system, &mut app.swash_cache, pixels, stride, w, h, &format!("  {}", items[idx]), y_pos, 0xCC, 0xCC, 0xCC);
        }
        y_pos += line_h;
    }

    let pool = app.shm.create_pool(fd.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(0, w, h, stride, wayland_client::protocol::wl_shm::Format::Xrgb8888, qh, ());
    buffer
}

// --- Wayland 事件分发实现 ---

impl Dispatch<WlRegistry, GlobalListContents> for App { fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as wayland_client::Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WlCompositor, ()> for App { fn event(_: &mut Self, _: &WlCompositor, _: <WlCompositor as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WlSurface, ()> for App { fn event(_: &mut Self, _: &WlSurface, _: <WlSurface as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<ZwlrLayerShellV1, ()> for App { fn event(_: &mut Self, _: &ZwlrLayerShellV1, _: <ZwlrLayerShellV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WlShm, ()> for App { fn event(_: &mut Self, _: &WlShm, _: <WlShm as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wayland_client::protocol::wl_shm_pool::WlShmPool, ()> for App { fn event(_: &mut Self, _: &wayland_client::protocol::wl_shm_pool::WlShmPool, _: <wayland_client::protocol::wl_shm_pool::WlShmPool as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WlBuffer, ()> for App { fn event(_: &mut Self, _: &WlBuffer, _: <WlBuffer as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

impl Dispatch<ZwpTextInputManagerV3, ()> for App { fn event(_: &mut Self, _: &ZwpTextInputManagerV3, _: <ZwpTextInputManagerV3 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

// 输入法事件处理
impl Dispatch<ZwpTextInputV3, ()> for App {
    fn event(state: &mut Self, _proxy: &ZwpTextInputV3, event: <ZwpTextInputV3 as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::Event;
        match event {
            Event::Enter { .. } => {
                if let Some(ti) = &state.text_input {
                    ti.enable();
                    ti.set_content_type(ContentHint::None, ContentPurpose::Normal);
                    ti.commit();
                }
            }
            Event::Leave { .. } => {
                if let Some(ti) = &state.text_input {
                    ti.disable();
                    ti.commit();
                }
                state.preedit.clear();
                state.need_redraw = true;
            }
            Event::PreeditString { text, .. } => {
                // text 是 Option<String>，unwrap_or_default 会处理 None 的情况
                state.preedit = text.unwrap_or_default();
                state.need_redraw = true;
            }
            Event::CommitString { text, .. } => {
                if let Some(t) = text {
                    state.query.push_str(&t);
                }
                state.preedit.clear();
                state.update_filter();
            }
            Event::Done { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for App {
    fn event(state: &mut Self, proxy: &ZwlrLayerSurfaceV1, event: <ZwlrLayerSurfaceV1 as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, qh: &QueueHandle<Self>) {
        use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event;
        match event {
            Event::Configure { serial, width, height, .. } => {
                proxy.ack_configure(serial);
                let w = if width == 0 { 800 } else { width };
                let h = if height == 0 { 200 } else { height };
                let buffer = create_shm_buffer(state, qh, w, h);
                if let Some(surface) = &state.surface {
                    surface.attach(Some(&buffer), 0, 0);
                    surface.damage(0, 0, w as i32, h as i32);
                    surface.commit();
                }
            }
            Event::Closed => { state.exit_code = Some(1); }
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, ()> for App { fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

impl Dispatch<WlKeyboard, ()> for App {
    fn event(state: &mut Self, _proxy: &WlKeyboard, event: <WlKeyboard as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        use wl_keyboard::Event;
        match event {
            Event::Enter { .. } => {
                // 键盘获得焦点时，主动启用输入法
                if let Some(ti) = &state.text_input {
                    ti.enable();
                    ti.set_content_type(ContentHint::None, ContentPurpose::Normal);
                    ti.commit();
                }
            }
            Event::Leave { .. } => {
                if let Some(ti) = &state.text_input {
                    ti.disable();
                    ti.commit();
                }
                state.preedit.clear();
                state.need_redraw = true;
            }
            Event::Key { state: key_state, key, .. } => {
                if key_state == wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed) {
                    let key_char = match key {
                        16 => Some('q'), 17 => Some('w'), 18 => Some('e'), 19 => Some('r'), 20 => Some('t'),
                        21 => Some('y'), 22 => Some('u'), 23 => Some('i'), 24 => Some('o'), 25 => Some('p'),
                        30 => Some('a'), 31 => Some('s'), 32 => Some('d'), 33 => Some('f'), 34 => Some('g'),
                        35 => Some('h'), 36 => Some('j'), 37 => Some('k'), 38 => Some('l'),
                        44 => Some('z'), 45 => Some('x'), 46 => Some('c'), 47 => Some('v'), 48 => Some('b'),
                        49 => Some('n'), 50 => Some('m'),
                        57 => Some(' '),
                        _ => None,
                    };

                    match key {
                        1 => state.exit_code = Some(1),
                        28 => {
                            if let Some(&idx) = state.filtered_items.get(state.selected_idx) {
                                state.output = Some(state.state.as_ref().unwrap().items[idx].clone());
                            }
                            state.exit_code = Some(0);
                        }
                        14 => {
                            state.query.pop();
                            state.update_filter();
                        }
                        103 => {
                            if state.selected_idx > 0 {
                                state.selected_idx -= 1;
                                state.need_redraw = true;
                            }
                        }
                        108 => {
                            if state.selected_idx + 1 < state.filtered_items.len() {
                                state.selected_idx += 1;
                                state.need_redraw = true;
                            }
                        }
                        _ => {
                            if let Some(c) = key_char {
                                state.query.push(c);
                                state.update_filter();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
