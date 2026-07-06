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

use crate::config::Config;
use crate::keyboard::get_char;
use crate::render::Renderer;
use crate::state::State;

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
    pub width: i32,            // 物理宽度
    pub height: i32,           // 物理高度
    pub logical_width: i32,    // 新增：逻辑宽度
    pub logical_height: i32,   // 新增：逻辑高度
    pub fractional_scale: u32, // 新增：缩放比例 (120 = 1.0x, 180 = 1.5x)
    pub fractional_scale_manager: Option<WpFractionalScaleManagerV1>, // 新增
    pub viewporter: Option<WpViewporter>,                             // 新增
    pub viewport: Option<WpViewport>,                                 // 新增
    pub fractional_scale_obj: Option<WpFractionalScaleV1>,            // 新增
    pub pointer_y: f64,        // 新增：记录鼠标逻辑 Y 坐标
    pub axis_accumulator: f64, // 新增：滚轮累加值
}

pub fn run(items: Vec<String>, config: Config) -> io::Result<Option<(usize, String)>> {
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

    // 尝试绑定分数缩放和视口协议 (如果不支持则为 None，不影响运行)
    let fractional_scale_manager: Option<WpFractionalScaleManagerV1> = globals.bind(&qh, 1..=1, ()).ok();
    let viewporter: Option<WpViewporter> = globals.bind(&qh, 1..=1, ()).ok();

    let mut app = App {
        state: State::new(items, &config),
        renderer: Renderer::new(),
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
        fractional_scale: 120, // 默认 1.0x
        fractional_scale_manager,
        viewporter,
        viewport: None,
        fractional_scale_obj: None,
        pointer_y: 0.0,        // 新增
        axis_accumulator: 0.0, // 新增
    };

    let surface = app.compositor.create_surface(&qh, ());
    app.surface = Some(surface.clone());

    // 为 surface 绑定分数缩放和视口对象
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
    // 加上和渲染时一样的 6.0 * scale 行间距
    let phys_line_h = (base_phys_line_h + 6.0 * scale).ceil() as u32;
    let visible_count = (app.config.lines as usize).min(app.state.items.len());
    let total_rows = (visible_count + 1) as u32;
    let phys_height = total_rows * phys_line_h;
    let height = (phys_height as f32 / scale).ceil() as u32;

    if app.config.width == 0 {
        layer_surface.set_anchor(Anchor::Top | Anchor::Left | Anchor::Right);
        layer_surface.set_size(0, height);
    } else {
        layer_surface.set_anchor(Anchor::Top | Anchor::Left);
        layer_surface.set_size(app.config.width, height);
    }

    layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    surface.commit();

    while !app.configured {
        if let Err(e) = queue.blocking_dispatch(&mut app) {
            eprintln!("Wayland dispatch error during initial configure: {:?}", e);
            std::process::exit(3);
        }
    }

    let _keyboard = seat.get_keyboard(&qh, ());
    let _pointer = seat.get_pointer(&qh, ()); // 新增：获取鼠标指针

    loop {
        if let Some(code) = app.state.exit_code {
            if code == 0 {
                let text = app.state.output.clone().unwrap_or_default();
                let idx = app.state.selected_original_idx.unwrap_or(usize::MAX);
                return Ok(Some((idx, text)));
            }
            return Ok(None);
        }

        if app.state.need_redraw && app.configured {
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
                    app.renderer.draw_frame(pixels, w, h, scale, &app.state, &app.config);
                    slot.busy = true;

                    if let Some(surface) = &app.surface {
                        surface.attach(Some(&slot.buffer), 0, 0);
                        surface.damage(0, 0, w, h);

                        // 新增：每次 commit 前强调逻辑尺寸，确保合成器正确缩放
                        if let Some(viewport) = &app.viewport {
                            viewport.set_destination(app.logical_width, app.logical_height);
                        }

                        surface.commit();
                    }
                    app.state.need_redraw = false;
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
    let buffer = pool.create_buffer(0, width, height, stride, wayland_client::protocol::wl_shm::Format::Xrgb8888, qh, idx);
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
impl Dispatch<WlSurface, ()> for App { fn event(_: &mut Self, _: &WlSurface, _: <WlSurface as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<ZwlrLayerShellV1, ()> for App { fn event(_: &mut Self, _: &ZwlrLayerShellV1, _: <ZwlrLayerShellV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WlShm, ()> for App { fn event(_: &mut Self, _: &WlShm, _: <WlShm as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wayland_client::protocol::wl_shm_pool::WlShmPool, ()> for App { fn event(_: &mut Self, _: &wayland_client::protocol::wl_shm_pool::WlShmPool, _: <wayland_client::protocol::wl_shm_pool::WlShmPool as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

impl Dispatch<WlBuffer, usize> for App {
    fn event(state: &mut Self, _proxy: &WlBuffer, event: <WlBuffer as wayland_client::Proxy>::Event, data: &usize, _: &Connection, _: &QueueHandle<Self>) {
        if let wayland_client::protocol::wl_buffer::Event::Release = event {
            if let Some(Some(slot)) = state.buffers.get_mut(*data) {
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
                state.state.clear_preedit();
            }
            Event::PreeditString { text, .. } => {
                state.state.set_preedit(&text.unwrap_or_default());
            }
            Event::CommitString { text, .. } => {
                if let Some(t) = text {
                    state.state.commit_str(&t);
                }
            }
            Event::Done { .. } => {}
            _ => {}
        }
    }
}

// 新增：处理分数缩放事件
impl Dispatch<WpFractionalScaleManagerV1, ()> for App { fn event(_: &mut Self, _: &WpFractionalScaleManagerV1, _: <WpFractionalScaleManagerV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WpFractionalScaleV1, ()> for App {
    fn event(state: &mut Self, _proxy: &WpFractionalScaleV1, event: <WpFractionalScaleV1 as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::Event as FractionalScaleEvent;
        if let FractionalScaleEvent::PreferredScale { scale } = event {
            if state.fractional_scale != scale {
                state.fractional_scale = scale;
                let scale_f = scale as f32 / 120.0;
                // 根据逻辑尺寸重新计算物理尺寸
                state.width = ((state.logical_width as f32 * scale_f).round() as i32).max(1);
                state.height = ((state.logical_height as f32 * scale_f).round() as i32).max(1);
                // 强制销毁旧 Buffer，主循环会重新分配
                for i in 0..2 {
                    state.buffers[i] = None;
                }
                state.state.need_redraw = true;
            }
        }
    }
}

// 新增：处理视口协议事件
impl Dispatch<WpViewporter, ()> for App { fn event(_: &mut Self, _: &WpViewporter, _: <WpViewporter as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<WpViewport, ()> for App { fn event(_: &mut Self, _: &WpViewport, _: <WpViewport as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

impl Dispatch<ZwlrLayerSurfaceV1, ()> for App {
    fn event(state: &mut Self, proxy: &ZwlrLayerSurfaceV1, event: <ZwlrLayerSurfaceV1 as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event;
        match event {
            Event::Configure { serial, width, height, .. } => {
                proxy.ack_configure(serial);
                state.configured = true;

                let w_u32 = if width == 0 { state.config.width } else { width };
                let w_u32 = if w_u32 == 0 { 800 } else { w_u32 };

                let scale = state.fractional_scale as f32 / 120.0;
                let phys_font_size = state.config.font_size * scale;
                let base_phys_line_h = state.renderer
                    .measure_line_height(phys_font_size, &state.config.font);
                // 加上和渲染时一样的 6.0 * scale 行间距
                let phys_line_h = (base_phys_line_h + 6.0 * scale).ceil() as i32;
                let visible_count = (state.config.lines as usize).min(state.state.items.len());
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
                    state.renderer.draw_frame(pixels, state.width, state.height, scale, &state.state, &state.config);
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
                state.state.clear_preedit();
            }
            Event::Modifiers { mods_depressed, .. } => {
                state.shift_pressed = (mods_depressed & 0x1) != 0 || (mods_depressed & 0x10) != 0;
                state.ctrl_pressed = (mods_depressed & 0x4) != 0;
            }
            Event::Key { state: key_state, key, .. } => {
                if key_state == wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed) {
                    if state.ctrl_pressed {
                        match key {
                            46 => { state.state.cancel(); return; }     // Ctrl + C
                            22 => { state.state.clear_query(); return; } // Ctrl + U
                            17 => { state.state.delete_word(); return; } // Ctrl + W
                            25 => { state.state.move_up(); return; }     // Ctrl + P
                            49 => { state.state.move_down(); return; }   // Ctrl + N
                            _ => {}
                        }
                        return;
                    }

                    match key {
                        29 | 42 | 54 | 56 | 97 | 100 | 125 | 126 => return, // 忽略修饰键
                        1  => { state.state.cancel(); return; }      // Esc
                        14 => { state.state.backspace(); return; }   // Bksp
                        15 => {
                            // Tab：只有在开启多选模式时才标记
                            if state.config.multi_select {
                                state.state.toggle_mark();
                            }
                            return;
                        }
                        28 => {
                            // Enter：传入是否多选模式
                            state.state.select_current(state.config.multi_select);
                            return;
                        }
                        103 => { state.state.move_up(); return; }    // Up
                        108 => { state.state.move_down(); return; }  // Down
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
                state.pointer_y = surface_y; // 记录最新的鼠标 Y 坐标
            }
            Event::Button { button, state: btn_state, .. } => {
                if btn_state == wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed) {
                    if button == 0x110 { // 0x110 = BTN_LEFT (左键)
                        let scale = state.fractional_scale as f32 / 120.0;
                        let phys_font_size = state.config.font_size * scale;
                        let base_phys_line_h = state.renderer.measure_line_height(phys_font_size, &state.config.font);
                        let phys_line_h = (base_phys_line_h + 6.0 * scale).ceil() as f32;

                        if phys_line_h > 0.0 {
                            // 因为使用了 viewport，鼠标传入的是逻辑坐标，我们要除以逻辑行高
                            let logical_line_h = phys_line_h / scale;
                            let clicked_row = (state.pointer_y / logical_line_h as f64).floor() as usize;

                            if clicked_row >= 1 { // 0 是输入框，>=1 是列表项
                                let target_idx = state.state.scroll_offset + clicked_row - 1;
                                if target_idx < state.state.filtered_items.len() {
                                    state.state.selected_idx = target_idx;
                                    state.state.select_current(state.config.multi_select);
                                }
                            }
                        }
                    } else if button == 0x111 { // 0x111 = BTN_RIGHT (右键)
                        // 右键多选逻辑 (仅在 -m 模式下生效)
                        if state.config.multi_select {
                            let scale = state.fractional_scale as f32 / 120.0;
                            let phys_font_size = state.config.font_size * scale;
                            let base_phys_line_h = state.renderer.measure_line_height(phys_font_size, &state.config.font);
                            let phys_line_h = (base_phys_line_h + 6.0 * scale).ceil() as f32;

                            if phys_line_h > 0.0 {
                                let logical_line_h = phys_line_h / scale;
                                let clicked_row = (state.pointer_y / logical_line_h as f64).floor() as usize;

                                if clicked_row >= 1 {
                                    let target_idx = state.state.scroll_offset + clicked_row - 1;
                                    if target_idx < state.state.filtered_items.len() {
                                        state.state.selected_idx = target_idx;
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
                    // value120 通常为 120 或 -120
                    let steps = (value120 / 120).abs() as usize;
                    let lines_per_step = 3; // 每格滚动 3 行

                    if value120 > 0 {
                        for _ in 0..(steps * lines_per_step) {
                            state.state.move_down();
                        }
                    } else if value120 < 0 {
                        for _ in 0..(steps * lines_per_step) {
                            state.state.move_up();
                        }
                    }
                }
            }
            // 保留旧的 Axis 事件作为兼容（某些老设备/触控板可能不发 120 信号）
            Event::Axis { axis, value, .. } => {
                if axis == wayland_client::WEnum::Value(wl_pointer::Axis::VerticalScroll) {
                    state.axis_accumulator += value;
                    let threshold = 15.0;
                    if state.axis_accumulator > threshold {
                        state.state.move_down();
                        state.state.move_down();
                        state.state.move_down(); // 同样改为 3 行
                        state.axis_accumulator = 0.0;
                    } else if state.axis_accumulator < -threshold {
                        state.state.move_up();
                        state.state.move_up();
                        state.state.move_up(); // 3 行
                        state.axis_accumulator = 0.0;
                    }
                }
            }
            _ => {}
        }
    }
}
