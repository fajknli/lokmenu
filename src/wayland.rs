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
    wl_registry::WlRegistry, wl_seat::WlSeat, wl_shm::WlShm, wl_surface::WlSurface,
};

use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{ContentHint, ContentPurpose, ZwpTextInputV3},
};

use crate::config::Config;
use crate::keyboard::get_char;
use crate::render::Renderer;
use crate::state::State;

// 修复 P0 #2：管理 mmap 生命周期
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

// 修复 P1 #3：管理 WlBuffer 生命周期与状态
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
    pub buffers: [Option<BufferSlot>; 2], // 双缓冲池
    pub configured: bool, // 新增：标记是否已收到 Configure 事件
    pub width: i32,       // 新增：保存当前配置的宽度
    pub height: i32,      // 新增：保存当前配置的高度
}

pub fn run(items: Vec<String>, config: Config) -> io::Result<Option<(usize, String)>> {
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
        buffers: [None, None],
        configured: false, // 初始化为 false
        width: 0,
        height: 0,
    };

    let surface = app.compositor.create_surface(&qh, ());
    app.surface = Some(surface.clone());

    let text_input = app.text_input_manager.get_text_input(&seat, &qh, ());
    app.text_input = Some(text_input);

    let layer_surface = app.layer_shell.get_layer_surface(
        &surface, None, Layer::Overlay, "lok".to_string(), &qh, (),
    );

    layer_surface.set_anchor(Anchor::Top | Anchor::Left);
    layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    let line_h = (app.config.font_size * 1.5).ceil() as u32;
    layer_surface.set_size(app.config.width, (app.config.lines + 1) * line_h);

    surface.commit();

    // 修复：阻塞等待第一个 Configure 事件，确保在 attach buffer 前完成协议握手
    while !app.configured {
        if let Err(e) = queue.blocking_dispatch(&mut app) {
            eprintln!("Wayland dispatch error during initial configure: {:?}", e);
            std::process::exit(3);
        }
    }

    let _keyboard = seat.get_keyboard(&qh, ());

    loop {
        // 1. 检查是否需要退出
        if let Some(code) = app.state.exit_code {
            if code == 0 {
                if let Some(idx) = app.state.selected_original_idx {
                    let text = app.state.output.clone().unwrap_or_default();
                    return Ok(Some((idx, text)));
                }
            }
            return Ok(None);
        }

        // 2. 如果已经配置完毕且需要重绘，则执行渲染逻辑
        if app.state.need_redraw && app.configured {
            let w = app.width;
            let h = app.height;

            // 寻找空闲的 Buffer 槽位
            let mut slot_idx = None;
            for i in 0..app.buffers.len() {
                if let Some(slot) = &app.buffers[i] {
                    if !slot.busy && slot.width == w && slot.height == h {
                        slot_idx = Some(i);
                        break;
                    }
                }
            }

            // 如果没有匹配的空闲槽位，寻找可复用或空的槽位重建
            if slot_idx.is_none() {
                for i in 0..app.buffers.len() {
                    let need_recreate = match &app.buffers[i] {
                        None => true,
                        Some(slot) => !slot.busy && (slot.width != w || slot.height != h),
                    };
                    if need_recreate {
                        app.buffers[i] = None; // 触发 Drop 回收旧的
                        if create_shm_buffer(&mut app, &qh, w, h, i) {
                            slot_idx = Some(i);
                            break;
                        }
                    }
                }
            }

            // 渲染并挂载
            if let Some(idx) = slot_idx {
                if let Some(slot) = app.buffers[idx].as_mut() {
                    let pixels = unsafe { std::slice::from_raw_parts_mut(slot.shm.ptr, slot.shm.size) };
                    app.renderer.draw_frame(pixels, w, h, &app.state, &app.config);
                    slot.busy = true;

                    if let Some(surface) = &app.surface {
                        surface.attach(Some(&slot.buffer), 0, 0);
                        surface.damage(0, 0, w, h);
                        surface.commit();
                    }
                    app.state.need_redraw = false;
                }
            }
            // 如果所有槽位都在忙，则保留 need_redraw = true，等待 compositor release 事件后再次重绘
        }

        // 3. 阻塞等待新的 Wayland 事件（在此之前会自动 flush 上面的 attach/commit 请求）
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
    // 将 idx 作为 user_data 绑定到 buffer 事件
    let buffer = pool.create_buffer(0, width, height, stride, wayland_client::protocol::wl_shm::Format::Xrgb8888, qh, idx);
    pool.destroy(); // 销毁 pool，buffer 会自己保留引用

    app.buffers[idx] = Some(BufferSlot {
        buffer,
        shm,
        busy: false, // 刚创建，即将使用
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

// 修复 P1 #3：监听 WlBuffer 释放事件
impl Dispatch<WlBuffer, usize> for App {
    fn event(state: &mut Self, _proxy: &WlBuffer, event: <WlBuffer as wayland_client::Proxy>::Event, data: &usize, _: &Connection, _: &QueueHandle<Self>) {
        if let wayland_client::protocol::wl_buffer::Event::Release = event {
            if let Some(Some(slot)) = state.buffers.get_mut(*data) {
                slot.busy = false;
                // 删除 need_redraw = true，避免重复绘制相同内容导致画面抖动
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

impl Dispatch<ZwlrLayerSurfaceV1, ()> for App {
    fn event(state: &mut Self, proxy: &ZwlrLayerSurfaceV1, event: <ZwlrLayerSurfaceV1 as wayland_client::Proxy>::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<Self>) {
        use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event;
        match event {
            Event::Configure { serial, width, height, .. } => {
                proxy.ack_configure(serial);
                state.configured = true;

                let w_u32 = if width == 0 { state.config.width } else { width };
                let line_h = (state.config.font_size * 1.5).ceil() as u32;
                let h_u32 = if height == 0 { (state.config.lines + 1) * line_h } else { height };

                state.width = w_u32 as i32;
                state.height = h_u32 as i32;

                // 预分配两个 buffer slot，主循环直接用
                for i in 0..2 {
                    if state.buffers[i].as_ref().map_or(true, |s| s.width != state.width || s.height != state.height) {
                        state.buffers[i] = None;
                        create_shm_buffer(state, _qh, state.width, state.height, i);
                    }
                }

                state.state.need_redraw = true;
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
            }
            Event::Key { state: key_state, key, .. } => {
                if key_state == wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed) {
                    match key {
                        1 | 14 | 15 | 28 | 103 | 108 => { /* Esc, Bksp, Tab, Enter, Up, Down */ }
                        29 | 42 | 54 | 56 | 97 | 100 | 125 | 126 => return,
                        _ => {}
                    }

                    match key {
                        1  => { state.state.cancel(); return; }
                        14 => { state.state.backspace(); return; }
                        28 => { state.state.select_current(); return; }
                        103 => { state.state.move_up(); return; }
                        108 => { state.state.move_down(); return; }
                        15 => { state.state.cancel(); return; }
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
