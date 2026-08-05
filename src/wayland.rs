// src/wayland.rs

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
    wl_pointer::{self, WlPointer}, wl_registry::WlRegistry, wl_seat::WlSeat, wl_shm::WlShm, wl_surface::WlSurface,
};

use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{ContentHint, ContentPurpose, ZwpTextInputV3},
};

// 新增：引入分数缩放和视口协议
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::WpFractionalScaleV1,
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewporter::WpViewporter,
    wp_viewport::WpViewport,
};

use crate::config::{Config, WindowAnchor};
use crate::keyboard::get_char;
use crate::render::Renderer;
use crate::state::State;

#[derive(Debug)]
struct SlotId(pub usize);

pub struct ShmBuffer {
    ptr: *mut u8,
    size: usize,
    _fd: OwnedFd,
}

impl Drop for ShmBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.size); }
        }
    }
}

pub struct BufferSlot {
    buffer: WlBuffer,
    shm: ShmBuffer,
    busy: bool,
    width: i32,
    height: i32,
}

pub struct App {
    pub state: State,
    pub renderer: Renderer,
    pub config: Config,
    pub layer_shell: ZwlrLayerShellV1,
    pub compositor: WlCompositor,
    pub shm: WlShm,
    pub surface: Option<WlSurface>,
    pub text_input_manager: ZwpTextInputManagerV3,
    pub text_input: Option<ZwpTextInputV3>,
    pub shift_pressed: bool,
    pub ctrl_pressed: bool,
    pub buffers: [Option<BufferSlot>; 2],
    pub configured: bool,
    pub width: i32,
    pub height: i32,
    pub logical_width: i32,
    pub logical_height: i32,
    pub fractional_scale: u32,
    pub fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    pub viewporter: Option<WpViewporter>,
    pub viewport: Option<WpViewport>,
    pub fractional_scale_obj: Option<WpFractionalScaleV1>,
    pub pointer_y: f64,
    pub axis_accumulator: f64,
    pub cursor_x: i32,  // 新增
    pub cursor_y: i32,  // 新增
}

pub fn run(items: Vec<String>, config: Config) -> io::Result<Option<(Vec<usize>, String)>> {
    let conn = Connection::connect_to_env().map_err(|e| {
        io::Error::new(io::ErrorKind::ConnectionRefused, format!("Wayland connect failed: {:?}", e))
    })?;

    let (globals, mut queue) = registry_queue_init::<App>(&conn).map_err(|e| {
        io::Error::new(io::ErrorKind::ConnectionRefused, format!("registry init failed: {:?}", e))
    })?;
    let qh = queue.handle();

    let compositor: WlCompositor = globals.bind(&qh, 1..=4, ()).map_err(|_| {
        io::Error::new(io::ErrorKind::NotFound, "Compositor (wl_compositor) not supported")
    })?;
    let layer_shell: ZwlrLayerShellV1 = globals.bind(&qh, 1..=4, ()).map_err(|_| {
        io::Error::new(io::ErrorKind::NotFound, "wlr-layer-shell protocol not supported")
    })?;
    let shm: WlShm = globals.bind(&qh, 1..=1, ()).map_err(|_| {
        io::Error::new(io::ErrorKind::NotFound, "wl_shm not supported")
    })?;
    let seat: WlSeat = globals.bind(&qh, 1..=4, ()).map_err(|_| {
        io::Error::new(io::ErrorKind::NotFound, "wl_seat not supported")
    })?;
    let text_input_manager: ZwpTextInputManagerV3 = globals.bind(&qh, 1..=1, ()).map_err(|_| {
        io::Error::new(io::ErrorKind::NotFound, "text-input-v3 protocol not supported")
    })?;

    let fractional_scale_manager: Option<WpFractionalScaleManagerV1> = globals.bind(&qh, 1..=1, ()).ok();
    let viewporter: Option<WpViewporter> = globals.bind(&qh, 1..=1, ()).ok();

    let mut app = App {
        state: State::new(items, &config),
        renderer: Renderer::new(&config.font),
        config,
        layer_shell,
        compositor,
        shm,
        surface: None,
        text_input_manager,
        text_input: None,
        shift_pressed: false,
        ctrl_pressed: false,
        buffers: [None, None],
        configured: false,
        width: 0,
        height: 0,
        logical_width: 0,
        logical_height: 0,
        fractional_scale: 120,
        fractional_scale_manager,
        viewporter,
        viewport: None,
        fractional_scale_obj: None,
        pointer_y: 0.0,
        axis_accumulator: 0.0,
        cursor_x: 0,     // 新增
        cursor_y: 0,     // 新增
    };

    let surface = app.compositor.create_surface(&qh, ());
    app.surface = Some(surface.clone());

    if let Some(fsm) = &app.fractional_scale_manager {
        let fs = fsm.get_fractional_scale(&surface, &qh, ());
        app.fractional_scale_obj = Some(fs);
    }
    if let Some(vp) = &app.viewporter {
        let viewport = vp.get_viewport(&surface, &qh, ());
        app.viewport = Some(viewport);
    }

    let text_input = app.text_input_manager.get_text_input(&seat, &qh, ());
    app.text_input = Some(text_input);

    let layer_surface = app.layer_shell.get_layer_surface(
        &surface, None, Layer::Overlay, "lok".to_string(), &qh, (),
    );

    let scale = app.fractional_scale as f32 / 120.0;
    let phys_font_size = app.config.font_size * scale;
    let base_phys_line_h = app.renderer
        .measure_line_height(phys_font_size, &app.config.font);
    let phys_line_h = (base_phys_line_h * 1.15).ceil() as u32;
    let visible_count = if app.config.password { 0 } else { (app.config.lines as usize).min(app.state.items.len()) };
    let total_rows = (visible_count + 1) as u32;
    let phys_height = total_rows * phys_line_h;
    let height = (phys_height as f32 / scale).ceil() as u32;


    let layer_anchor = match app.config.anchor {
        WindowAnchor::Top          => Anchor::Top | Anchor::Left | Anchor::Right,
        WindowAnchor::TopLeft      => Anchor::Top | Anchor::Left,
        WindowAnchor::TopCenter    => Anchor::Top,
        WindowAnchor::TopRight     => Anchor::Top | Anchor::Right,
        WindowAnchor::Bottom       => Anchor::Bottom | Anchor::Left | Anchor::Right,
        WindowAnchor::BottomLeft   => Anchor::Bottom | Anchor::Left,
        WindowAnchor::BottomCenter => Anchor::Bottom,
        WindowAnchor::BottomRight  => Anchor::Bottom | Anchor::Right,
    };

    layer_surface.set_anchor(layer_anchor);

    let is_full_width = app.config.anchor.is_full_width();
    let request_width = if is_full_width && app.config.width == 0 {
        0
    } else if app.config.width == 0 {
        800
    } else {
        app.config.width
    };
    layer_surface.set_size(request_width, height);

    layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    surface.commit();

    while !app.configured {
        if let Err(e) = queue.blocking_dispatch(&mut app) {
            eprintln!("Wayland dispatch error during initial configure: {:?}", e);
            std::process::exit(3);
        }
    }

    let _keyboard = seat.get_keyboard(&qh, ());
    let _pointer = seat.get_pointer(&qh, ());

    loop {
        if let Some(code) = app.state.exit_code {
            if code == 0 {
                let text = app.state.output.clone().unwrap_or_default();
                let indices = app.state.output_indices.clone();
                return Ok(Some((indices, text)));
            }
            return Ok(None);
        }

        if app.state.need_redraw && app.configured {
            // ↓↓↓ 新增：快路径检查开始 ↓↓↓
            let wayland_fd = conn.backend().poll_fd().as_raw_fd();
            let mut peek = [
                libc::pollfd { fd: wayland_fd, events: libc::POLLIN, revents: 0 },
            ];
            // 超时时间为 0 的非阻塞 poll，仅仅是为了看一眼还有没有事件
            unsafe { libc::poll(peek.as_mut_ptr(), 1, 0); }

            if peek[0].revents & libc::POLLIN != 0 {
                // 队列里还有事件没处理完 → 跳过本次渲染，等下一轮循环
            } else {
                // 队列空了 → 安心渲染
                // ↓↓↓ 你原来的渲染逻辑全部放进这个 else 里 ↓↓↓
                let w = app.width;
                let h = app.height;
                let scale = app.fractional_scale as f32 / 120.0;

                let mut slot_idx = None;
                for i in 0..app.buffers.len() {
                    if let Some(slot) = &app.buffers[i] {
                        if !slot.busy && slot.width == w && slot.height == h {
                            slot_idx = Some(i);
                            break;
                        }
                    }
                }

                if slot_idx.is_none() {
                    for i in 0..app.buffers.len() {
                        let need_recreate = match &app.buffers[i] {
                            None => true,
                            Some(slot) => !slot.busy && (slot.width != w || slot.height != h),
                        };
                        if need_recreate {
                            app.buffers[i] = None;
                            if create_shm_buffer(&mut app, &qh, w, h, i) {
                                slot_idx = Some(i);
                                break;
                            }
                        }
                    }
                }

                if let Some(idx) = slot_idx {
                    if let Some(slot) = app.buffers[idx].as_mut() {
                        let pixels = unsafe { std::slice::from_raw_parts_mut(slot.shm.ptr, slot.shm.size) };
                        if let Some((cx, cy)) = app.renderer.draw_frame(pixels, w, h, scale, &app.state, &app.config) {
                            app.cursor_x = cx;
                            app.cursor_y = cy;
                        }
                        slot.busy = true;

                        if let Some(surface) = &app.surface {
                            surface.attach(Some(&slot.buffer), 0, 0);
                            surface.damage(0, 0, w, h);

                            if let Some(viewport) = &app.viewport {
                                viewport.set_destination(app.logical_width, app.logical_height);
                            }

                            surface.commit();
                        }
                        app.state.need_redraw = false;
                    }
                }
            }
        }

        match queue.blocking_dispatch(&mut app) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Wayland dispatch error: {:?}", e);
                std::process::exit(3);
            }
        }
    }
}

fn create_shm_buffer(app: &mut App, qh: &QueueHandle<App>, width: i32, height: i32, idx: usize) -> bool {
    let stride = width * 4;
    let size = (stride * height) as usize;

    let name = CString::new("lok-shm").unwrap();
    let fd = unsafe { libc::syscall(libc::SYS_memfd_create, name.as_ptr(), 0) };
    if fd == -1 { return false; }
    let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };

    if unsafe { libc::ftruncate(fd.as_raw_fd(), size as i64) } == -1 {
        return false;
    }

    let ptr = unsafe {
        libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd.as_raw_fd(), 0) as *mut u8
    };
    if ptr == libc::MAP_FAILED as *mut u8 { return false; }

    let shm = ShmBuffer { ptr, size, _fd: fd };

    let pool = app.shm.create_pool(shm._fd.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(
        0, width, height, stride,
        wayland_client::protocol::wl_shm::Format::Argb8888,
        qh,
        SlotId(idx)
    );
    pool.destroy();

    app.buffers[idx] = Some(BufferSlot {
        buffer,
        shm,
        busy: false,
        width,
        height,
    });
    true
}

// --- Wayland 事件分发实现 ---

impl Dispatch<WlRegistry, GlobalListContents> for App { fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as wayland_client::Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WlCompositor, ()> for App { fn event(_: &mut Self, _: &WlCompositor, _: <WlCompositor as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WlSurface, ()> for App {
    fn event(state: &mut Self, _: &WlSurface, event: <WlSurface as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        // 监听整数缩放变化 (某些不支持分数缩放的合成器会用这个)
        if let wayland_client::protocol::wl_surface::Event::PreferredBufferScale { factor } = event {
            let new_scale = (factor as u32).max(1) * 120; // 转换为统一的 120 倍数格式
            if state.fractional_scale != new_scale {
                state.fractional_scale = new_scale;
                let scale_f = new_scale as f32 / 120.0;
                state.width = ((state.logical_width as f32 * scale_f).round() as i32).max(1);
                state.height = ((state.logical_height as f32 * scale_f).round() as i32).max(1);
                // 销毁旧 buffer，下轮循环会自动重建
                for i in 0..2 {
                    state.buffers[i] = None;
                }
                state.state.need_redraw = true;
            }
        }
    }
}
impl Dispatch<ZwlrLayerShellV1, ()> for App { fn event(_: &mut Self, _: &ZwlrLayerShellV1, _: <ZwlrLayerShellV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WlShm, ()> for App { fn event(_: &mut Self, _: &WlShm, _: <WlShm as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wayland_client::protocol::wl_shm_pool::WlShmPool, ()> for App { fn event(_: &mut Self, _: &wayland_client::protocol::wl_shm_pool::WlShmPool, _: <wayland_client::protocol::wl_shm_pool::WlShmPool as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

impl Dispatch<WlBuffer, SlotId> for App {
    fn event(state: &mut Self, _proxy: &WlBuffer, event: <WlBuffer as wayland_client::Proxy>::Event, data: &SlotId, _: &Connection, _: &QueueHandle<Self>) {
        if let wayland_client::protocol::wl_buffer::Event::Release = event {
            // 通过 data.0 取出真正的索引
            if let Some(Some(slot)) = state.buffers.get_mut(data.0) {
                slot.busy = false;
            }
        }
    }
}

impl Dispatch<ZwpTextInputManagerV3, ()> for App { fn event(_: &mut Self, _: &ZwpTextInputManagerV3, _: <ZwpTextInputManagerV3 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

impl Dispatch<ZwpTextInputV3, ()> for App {
    fn event(state: &mut Self, _proxy: &ZwpTextInputV3, event: <ZwpTextInputV3 as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::Event;
        match event {
            Event::Enter { .. } => {
                if let Some(ti) = state.text_input.clone() {
                    ti.enable();
                    ti.set_content_type(ContentHint::None, ContentPurpose::Normal);
                    update_ime_cursor(&ti, state);
                    ti.commit();
                }
            }
            Event::Leave { .. } => {
                if let Some(ti) = &state.text_input {
                    ti.disable();
                    ti.commit();
                }
                state.state.clear_preedit();
            }
            Event::PreeditString { text, .. } => {
                state.state.set_preedit(&text.unwrap_or_default());
            }
            Event::CommitString { text, .. } => {
                if let Some(t) = text {
                    state.state.commit_str(&t);
                }
                if let Some(ti) = state.text_input.clone() {
                    update_ime_cursor(&ti, state);
                    ti.commit();
                }
            }
            Event::Done { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<WpFractionalScaleManagerV1, ()> for App { fn event(_: &mut Self, _: &WpFractionalScaleManagerV1, _: <WpFractionalScaleManagerV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WpFractionalScaleV1, ()> for App {
    fn event(state: &mut Self, _proxy: &WpFractionalScaleV1, event: <WpFractionalScaleV1 as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::Event as FractionalScaleEvent;
        if let FractionalScaleEvent::PreferredScale { scale } = event {
            if state.fractional_scale != scale {
                state.fractional_scale = scale;
                let scale_f = scale as f32 / 120.0;
                state.width = ((state.logical_width as f32 * scale_f).round() as i32).max(1);
                state.height = ((state.logical_height as f32 * scale_f).round() as i32).max(1);
                for i in 0..2 {
                    state.buffers[i] = None;
                }
                state.state.need_redraw = true;
            }
        }
    }
}

impl Dispatch<WpViewporter, ()> for App { fn event(_: &mut Self, _: &WpViewporter, _: <WpViewporter as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WpViewport, ()> for App { fn event(_: &mut Self, _: &WpViewport, _: <WpViewport as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

impl Dispatch<ZwlrLayerSurfaceV1, ()> for App {
    fn event(state: &mut Self, proxy: &ZwlrLayerSurfaceV1, event: <ZwlrLayerSurfaceV1 as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event;
        match event {
            Event::Configure { serial, width, height, .. } => {
                proxy.ack_configure(serial);
                state.configured = true;

                let w_u32 = if width == 0 {
                    if state.config.width == 0 { 800 } else { state.config.width }
                } else {
                    width
                };

                let scale = state.fractional_scale as f32 / 120.0;
                let phys_font_size = state.config.font_size * scale;
                let base_phys_line_h = state.renderer
                    .measure_line_height(phys_font_size, &state.config.font);
                let phys_line_h = (base_phys_line_h * 1.15).ceil() as i32;
                let visible_count = if state.config.password { 0 } else { (state.config.lines as usize).min(state.state.items.len()) };
                let line_count = (visible_count + 1) as i32;
                let phys_h = line_count * phys_line_h;
                let log_h = (phys_h as f32 / scale).ceil() as i32;

                state.logical_width = w_u32 as i32;
                state.logical_height = if height == 0 { log_h } else { height as i32 };
                state.width = (state.logical_width as f32 * scale).round() as i32;
                state.height = if height == 0 { phys_h } else { (height as f32 * scale).round() as i32 };

                if let Some(viewport) = &state.viewport {
                    viewport.set_destination(state.logical_width, state.logical_height);
                }

                for i in 0..2 {
                    if state.buffers[i].as_ref().map_or(true, |s| s.width != state.width || s.height != state.height) {
                        state.buffers[i] = None;
                        create_shm_buffer(state, _qh, state.width, state.height, i);
                    }
                }

                if let Some(slot) = state.buffers[0].as_mut() {
                    let pixels = unsafe { std::slice::from_raw_parts_mut(slot.shm.ptr, slot.shm.size) };
                    if let Some((cx, cy)) = state.renderer.draw_frame(pixels, state.width, state.height, scale, &state.state, &state.config) {
                        state.cursor_x = cx;
                        state.cursor_y = cy;
                    }
                    slot.busy = true;

                    if let Some(surface) = &state.surface {
                        surface.attach(Some(&slot.buffer), 0, 0);
                        surface.damage(0, 0, state.width, state.height);
                        surface.commit();
                    }
                }

                state.state.need_redraw = false;
            }
            Event::Closed => { state.state.cancel(); }
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
                if let Some(ti) = state.text_input.clone() {
                    ti.enable();
                    ti.set_content_type(ContentHint::None, ContentPurpose::Normal);
                    update_ime_cursor(&ti, state);
                    ti.commit();
                }
            }
            Event::Leave { .. } => {
                if let Some(ti) = &state.text_input {
                    ti.disable();
                    ti.commit();
                }
                state.state.clear_preedit();
            }
            Event::Modifiers { mods_depressed, .. } => {
                state.shift_pressed = (mods_depressed & 0x1) != 0;
                state.ctrl_pressed = (mods_depressed & 0x4) != 0;
            }
            Event::Key { state: key_state, key, .. } => {
                if key_state == wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed) {
                    if state.ctrl_pressed {
                        let is_bottom = state.config.anchor.is_bottom();
                        match key {
                            46 => { state.state.cancel(); return; }          // Ctrl + C
                            22 => { state.state.delete_to_start(); return; } // Ctrl + U (删到行首)
                            17 => { state.state.delete_word_left(); return; }// Ctrl + W
                            37 => { state.state.delete_to_end(); return; }   // Ctrl + K (删到行尾)
                            30 => { state.state.cursor_start(); return; }    // Ctrl + A
                            18 => { state.state.cursor_end(); return; }      // Ctrl + E
                            48 => { state.state.cursor_left(); return; }     // Ctrl + B
                            33 => { state.state.cursor_right(); return; }    // Ctrl + F
                            25 => {                                          // Ctrl + P
                                if is_bottom { state.state.move_down(); } else { state.state.move_up(); }
                                return;
                            }
                            49 => {                                          // Ctrl + N
                                if is_bottom { state.state.move_up(); } else { state.state.move_down(); }
                                return;
                            }
                            47 => {                                          // Ctrl + V (粘贴)
                                if let Ok(output) = std::process::Command::new("wl-paste")
                                    .arg("--no-newline")
                                    .output()
                                {
                                    if let Ok(text) = String::from_utf8(output.stdout) {
                                        if !text.is_empty() {
                                            state.state.commit_str(&text);
                                        }
                                    }
                                }
                                return;
                            }
                            _ => {}
                        }
                        return;
                    }

                    // 非组合键
                    match key {
                        29 | 42 | 54 | 56 | 97 | 100 | 125 | 126 => return, // 忽略 Shift, Ctrl, Alt 等单键
                        1  => { state.state.cancel(); return; }      // Esc
                        14 => { state.state.backspace(); return; }   // Bksp
                        15 => {
                            if state.config.multi_select {
                                state.state.toggle_mark();
                            }
                            return;
                        }
                        28 => {
                            state.state.select_current(state.config.multi_select);
                            return;
                        }
                        103 => {                                       // Up
                            let is_bottom = state.config.anchor.is_bottom();
                            if is_bottom { state.state.move_down(); } else { state.state.move_up(); }
                            return;
                        }
                        108 => {                                       // Down
                            let is_bottom = state.config.anchor.is_bottom();
                            if is_bottom { state.state.move_up(); } else { state.state.move_down(); }
                            return;
                        }
                        105 => { state.state.cursor_left(); return; }   // Left
                        106 => { state.state.cursor_right(); return; }  // Right
                        102 => { state.state.cursor_start(); return; }  // Home
                        107 => { state.state.cursor_end(); return; }    // End
                        _ => {}
                    }

                    if let Some(c) = get_char(key, state.shift_pressed) {
                        if !c.is_control() {
                            state.state.push_char(c);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for App {
    fn event(state: &mut Self, _proxy: &WlPointer, event: <WlPointer as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        use wl_pointer::Event;
        match event {
            Event::Enter { surface_y, .. } | Event::Motion { surface_y, .. } => {
                state.pointer_y = surface_y;
            }
            Event::Button { button, state: btn_state, .. } => {
                if btn_state == wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed) {
                    if button == 0x110 || button == 0x111 {
                        let is_bottom = state.config.anchor.is_bottom();
                        let scale = state.fractional_scale as f32 / 120.0;
                        let phys_font_size = state.config.font_size * scale;
                        let base_phys_line_h = state.renderer.measure_line_height(phys_font_size, &state.config.font);
                        let phys_line_h = (base_phys_line_h * 1.15).ceil() as f32;

                        if phys_line_h > 0.0 {
                            let logical_line_h = phys_line_h / scale;
                            let clicked_row = (state.pointer_y / logical_line_h as f64).floor() as usize;
                            let config_lines = state.config.lines as usize;
                            let visible_items = state.state.filtered_items.len().min(config_lines);

                            let target_idx_opt = if is_bottom {
                                // 底部模式：prompt 在最后一行，列表反序
                                let prompt_row = config_lines;
                                if clicked_row < prompt_row {
                                    let dist_from_prompt = prompt_row - 1 - clicked_row;
                                    if dist_from_prompt < visible_items {
                                        Some(state.state.scroll_offset + dist_from_prompt)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                // 顶部模式：prompt 在第 0 行
                                if clicked_row >= 1 {
                                    let item_row = clicked_row - 1;
                                    if item_row < visible_items {
                                        Some(state.state.scroll_offset + item_row)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            };

                            if let Some(target_idx) = target_idx_opt {
                                if target_idx < state.state.filtered_items.len() {
                                    state.state.selected_idx = target_idx;
                                    if button == 0x110 {
                                        state.state.select_current(state.config.multi_select);
                                    } else if button == 0x111 && state.config.multi_select {
                                        state.state.toggle_mark();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Event::AxisValue120 { axis, value120, .. } => {
                if axis == wayland_client::WEnum::Value(wl_pointer::Axis::VerticalScroll) {
                    let steps = (value120 / 120).abs() as usize;
                    let lines_per_step = 3;
                    let is_bottom = state.config.anchor.is_bottom();

                    if value120 > 0 {
                        if is_bottom { state.state.move_up_by(steps * lines_per_step); }
                        else { state.state.move_down_by(steps * lines_per_step); }
                    } else if value120 < 0 {
                        if is_bottom { state.state.move_down_by(steps * lines_per_step); }
                        else { state.state.move_up_by(steps * lines_per_step); }
                    }
                }
            }
            Event::Axis { axis, value, .. } => {
                if axis == wayland_client::WEnum::Value(wl_pointer::Axis::VerticalScroll) {
                    state.axis_accumulator += value;
                    let threshold = 15.0;
                    let is_bottom = state.config.anchor.is_bottom();
                    if state.axis_accumulator > threshold {
                        if is_bottom { state.state.move_up_by(3); }
                        else { state.state.move_down_by(3); }
                        state.axis_accumulator = 0.0;
                    } else if state.axis_accumulator < -threshold {
                        if is_bottom { state.state.move_down_by(3); }
                        else { state.state.move_up_by(3); }
                        state.axis_accumulator = 0.0;
                    }
                }
            }
            _ => {}
        }
    }
}

fn update_ime_cursor(ti: &ZwpTextInputV3, app: &mut App) {
    let scale = app.fractional_scale as f32 / 120.0;
    if scale <= 0.0 { return; }

    let logical_x = (app.cursor_x as f32 / scale).round() as i32;
    let logical_y = (app.cursor_y as f32 / scale).round() as i32;

    let font_size = app.config.font_size * scale;
    let base_h = app.renderer.measure_line_height(font_size, &app.config.font);
    let phys_line_h = (base_h * 1.15).ceil() as i32;
    let logical_h = (phys_line_h as f32 / scale).ceil() as i32;

    ti.set_cursor_rectangle(logical_x, logical_y, 2, logical_h);
}
