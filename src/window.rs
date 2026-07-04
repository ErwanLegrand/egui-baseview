use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::time::Instant;

use baseview::dpi::{LogicalSize, Size};
use baseview::{
    Event, EventStatus, Window, WindowContext, WindowHandle, WindowHandler, WindowOpenOptions,
    WindowScalePolicy, WindowSize,
};
use copypasta::ClipboardProvider;
use egui::{Pos2, Rect, Rgba, ViewportCommand, pos2, vec2};
use keyboard_types::Modifiers;
use raw_window_handle::HasWindowHandle;

use crate::{GraphicsConfig, renderer::Renderer};

#[cfg(feature = "nice-log")]
use nice_plug_core::{nice_error as error, nice_warn as warn};
#[cfg(all(feature = "tracing", not(feature = "nice-log")))]
use tracing::{error, warn};

#[derive(Debug, Clone)]
pub struct EguiWindowSettings {
    pub title: String,

    /// The size of the window
    pub size: Size,

    /// The dpi scaling policy
    pub scale_policy: WindowScalePolicy,

    pub graphics: GraphicsConfig,
}

impl EguiWindowSettings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tile(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn with_scale_policy(mut self, scale_policy: WindowScalePolicy) -> Self {
        self.scale_policy = scale_policy;
        self
    }

    pub fn with_graphics_config(mut self, config: GraphicsConfig) -> Self {
        self.graphics = config;
        self
    }
}

impl Default for EguiWindowSettings {
    fn default() -> Self {
        Self {
            title: String::new(),
            size: Size::Logical(LogicalSize {
                width: 300.0,
                height: 200.0,
            }),
            scale_policy: WindowScalePolicy::default(),
            graphics: GraphicsConfig::default(),
        }
    }
}

pub struct Queue<'a> {
    bg_color: Option<Rgba>,
    close_requested: bool,
    size: Option<Size>,
    key_capture: Option<KeyCapture>,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Queue<'a> {
    pub(crate) fn new() -> Self {
        Self {
            bg_color: None,
            close_requested: false,
            size: None,
            key_capture: None,
            _marker: PhantomData,
        }
    }

    /// Set the background color.
    pub fn bg_color(&mut self, bg_color: Rgba) {
        self.bg_color = Some(bg_color);
    }

    /// Set size of the window.
    pub fn resize(&mut self, size: impl Into<Size>) {
        self.size = Some(size.into());
    }

    /// Close the window.
    pub fn close_window(&mut self) {
        self.close_requested = true;
    }

    /// Set how to handle capturing key events from the host.
    pub fn set_key_capture(&mut self, key_capture: KeyCapture) {
        self.key_capture = Some(key_capture);
    }
}

/// Describes how to handle capturing key events from the host.
#[derive(Default, Debug, Clone, PartialEq)]
pub enum KeyCapture {
    #[default]
    /// All keys will be captured from the host.
    CaptureAll,
    /// No keys will be captured from the host.
    IgnoreAll,
    /// Only the given keys will be captured from the host.
    CaptureKeys(Vec<keyboard_types::Key>),
    /// All keys except the given ones will be captured from the host.
    IgnoreKeys(Vec<keyboard_types::Key>),
}

/// Handles an egui-baseview application
pub struct EguiWindow<State, U>
where
    State: 'static + Send,
    U: FnMut(&mut egui::Ui, &mut Queue, &mut State),
    U: 'static + Send,
{
    user_state: RefCell<State>,
    user_update: RefCell<U>,

    egui_ctx: RefCell<egui::Context>,
    viewport_id: egui::ViewportId,
    start_time: Instant,
    egui_input: RefCell<egui::RawInput>,
    pointer_pos_in_points: Cell<Option<egui::Pos2>>,
    current_cursor_icon: Cell<baseview::MouseCursor>,

    renderer: RefCell<Renderer>,

    clipboard_ctx: RefCell<Option<copypasta::ClipboardContext>>,

    bg_color: Cell<Rgba>,
    repaint_after: Cell<Option<Instant>>,
    key_capture: RefCell<KeyCapture>,
    pub window: WindowContext,
}

impl<State, U> EguiWindow<State, U>
where
    State: 'static + Send,
    U: FnMut(&mut egui::Ui, &mut Queue, &mut State),
    U: 'static + Send,
{
    fn new<B>(
        window: WindowContext,
        title: String,
        graphics_config: GraphicsConfig,
        mut build: B,
        update: U,
        mut state: State,
    ) -> EguiWindow<State, U>
    where
        B: FnMut(&egui::Context, &mut Queue, &mut State),
        B: 'static + Send,
    {
        let renderer = Renderer::new(window.clone(), graphics_config).unwrap_or_else(|err| {
            // TODO: better error log and not panicking, but that's gonna require baseview changes
            error!("oops! the gpu backend couldn't initialize! \n {err}");
            panic!("gpu backend failed to initialize: \n {err}")
        });
        let egui_ctx = egui::Context::default();

        let size = window.size();

        let screen_rect = Rect::from_min_size(
            Pos2::new(0f32, 0f32),
            vec2(size.logical.width as f32, size.logical.height as f32),
        );

        let viewport_info = egui::ViewportInfo {
            parent: None,
            title: Some(title),
            native_pixels_per_point: Some(size.scale_factor as f32),
            focused: Some(true),
            inner_rect: Some(screen_rect),
            ..Default::default()
        };
        let viewport_id = egui::ViewportId::default();

        let mut egui_input = egui::RawInput {
            max_texture_side: Some(renderer.max_texture_side()),
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let _ = egui_input.viewports.insert(viewport_id, viewport_info);

        let mut queue = Queue::new();

        (build)(&egui_ctx, &mut queue, &mut state);

        if let Some(new_size) = queue.size {
            window.resize(new_size);
        }

        let clipboard_ctx = match copypasta::ClipboardContext::new() {
            Ok(clipboard_ctx) => Some(clipboard_ctx),
            Err(e) => {
                error!("Failed to initialize clipboard: {}", e);
                None
            }
        };

        let start_time = Instant::now();

        Self {
            user_state: state.into(),
            user_update: update.into(),

            window,
            egui_ctx: egui_ctx.into(),
            viewport_id,
            start_time,
            egui_input: egui_input.into(),
            pointer_pos_in_points: None.into(),
            current_cursor_icon: baseview::MouseCursor::Default.into(),

            renderer: renderer.into(),
            bg_color: queue.bg_color.unwrap_or(Rgba::BLACK).into(),

            clipboard_ctx: clipboard_ctx.into(),

            repaint_after: Some(start_time).into(),
            key_capture: queue.key_capture.unwrap_or_default().into(),
        }
    }

    /// Open a new child window.
    ///
    /// * `parent` - The parent window.
    /// * `settings` - The settings of the window.
    /// * `state` - The initial state of your application.
    /// * `build` - Called once before the first frame. Allows you to do setup code and to
    ///   call `ctx.set_fonts()`. Optional.
    /// * `update` - Called before each frame. Here you should update the state of your
    ///   application and build the UI.
    pub fn open_parented<P, B>(
        parent: &P,
        settings: EguiWindowSettings,
        state: State,
        build: B,
        update: U,
    ) -> WindowHandle
    where
        P: HasWindowHandle,
        B: FnMut(&egui::Context, &mut Queue, &mut State),
        B: 'static + Send,
    {
        let options = WindowOpenOptions::new()
            .with_title(settings.title.clone())
            .with_size(settings.size)
            .with_scale_policy(settings.scale_policy);

        #[cfg(feature = "opengl")]
        let options = { options.with_gl_config(Some(settings.graphics.gl_config.clone())) };

        Window::open_parented(parent, options, move |window| -> EguiWindow<State, U> {
            EguiWindow::new(
                window,
                settings.title,
                settings.graphics,
                build,
                update,
                state,
            )
        })
    }

    /// Open a new window that blocks the current thread until the window is destroyed.
    ///
    /// * `settings` - The settings of the window.
    /// * `state` - The initial state of your application.
    /// * `build` - Called once before the first frame. Allows you to do setup code and to
    ///   call `ctx.set_fonts()`. Optional.
    /// * `update` - Called before each frame. Here you should update the state of your
    ///   application and build the UI.
    pub fn open_blocking<B>(settings: EguiWindowSettings, state: State, build: B, update: U)
    where
        B: FnMut(&egui::Context, &mut Queue, &mut State),
        B: 'static + Send,
    {
        let options = WindowOpenOptions::new()
            .with_title(settings.title.clone())
            .with_size(settings.size)
            .with_scale_policy(settings.scale_policy);

        #[cfg(feature = "opengl")]
        let options = { options.with_gl_config(Some(settings.graphics.gl_config.clone())) };

        Window::open_blocking(options, move |window| -> EguiWindow<State, U> {
            EguiWindow::new(
                window,
                settings.title,
                settings.graphics,
                build,
                update,
                state,
            )
        })
    }

    /// Update the pressed key modifiers when a mouse event has sent a new set of modifiers.
    fn update_modifiers(&self, modifiers: &Modifiers) {
        let mut egui_input = self.egui_input.borrow_mut();
        egui_input.modifiers.alt = !(*modifiers & Modifiers::ALT).is_empty();
        egui_input.modifiers.shift = !(*modifiers & Modifiers::SHIFT).is_empty();
        egui_input.modifiers.command = !(*modifiers & Modifiers::CONTROL).is_empty();
    }
}

impl<State, U> WindowHandler for EguiWindow<State, U>
where
    State: 'static + Send,
    U: FnMut(&mut egui::Ui, &mut Queue, &mut State),
    U: 'static + Send,
{
    fn on_frame(&self) {
        let egui_input = {
            let mut egui_input = self.egui_input.borrow_mut();
            egui_input.time = Some(self.start_time.elapsed().as_secs_f64());
            egui_input.screen_rect = Some(calculate_screen_rect(self.window.size()));
            egui_input.take()
        };

        //let mut repaint_requested = false;
        let mut queue = Queue::new();

        let mut full_output = {
            let egui_ctx = self.egui_ctx.borrow_mut();
            egui_ctx.run_ui(egui_input, |ui| {
                self.user_update.borrow_mut()(ui, &mut queue, &mut self.user_state.borrow_mut())
            })
        };

        if queue.close_requested {
            self.window.request_close();
        }

        // Prevent data from being allocated every frame by storing this
        // in a member field.

        let Some(viewport_output) = full_output.viewport_output.get(&self.viewport_id) else {
            // The main window was closed by egui.
            self.window.request_close();
            return;
        };

        for command in viewport_output.commands.iter() {
            match command {
                ViewportCommand::Close => {
                    self.window.request_close();
                }
                ViewportCommand::InnerSize(size) => self.window.resize(LogicalSize {
                    width: size.x.max(1.0),
                    height: size.y.max(1.0),
                }),
                _ => {}
            }
        }

        if let Some(size) = queue.size {
            self.window.resize(size);
        }

        let now = Instant::now();
        let do_repaint_now = if let Some(t) = self.repaint_after.get() {
            now >= t || viewport_output.repaint_delay.is_zero()
        } else {
            viewport_output.repaint_delay.is_zero()
        };

        if do_repaint_now {
            let size = self.window.size();
            self.renderer.borrow_mut().render(
                self.bg_color.get(),
                size.physical,
                size.scale_factor as f32,
                &mut self.egui_ctx.borrow_mut(),
                &mut full_output,
            );

            self.repaint_after.set(None);
        } else if let Some(repaint_after) = now.checked_add(viewport_output.repaint_delay) {
            // Schedule to repaint after the requested time has elapsed.
            self.repaint_after.set(Some(repaint_after));
        }

        for command in full_output.platform_output.commands {
            match command {
                egui::OutputCommand::CopyText(text) => {
                    if let Some(clipboard_ctx) = self.clipboard_ctx.borrow_mut().as_mut()
                        && let Err(err) = clipboard_ctx.set_contents(text)
                    {
                        error!("Copy/Cut error: {}", err);
                    }
                }
                egui::OutputCommand::CopyImage(_) => {
                    warn!("Copying images is not supported in egui_baseview.");
                }
                egui::OutputCommand::OpenUrl(open_url) => {
                    if let Err(err) = open::that_detached(&open_url.url) {
                        error!("Open error: {}", err);
                    }
                }
            }
        }

        let cursor_icon =
            crate::translate::translate_cursor_icon(full_output.platform_output.cursor_icon);
        if self.current_cursor_icon.get() != cursor_icon {
            self.current_cursor_icon.set(cursor_icon);

            self.window.set_mouse_cursor(cursor_icon);
        }

        // A temporary workaround for keyboard input not working sometimes.
        // See https://github.com/BillyDM/egui-baseview/issues/20
        #[cfg(feature = "keyboard_focus_workaround")]
        {
            if !full_output.platform_output.events.is_empty()
                || full_output.platform_output.ime.is_some()
            {
                window.focus();
            }
        }
    }

    fn resized(&self, new_size: WindowSize) {
        let screen_rect = calculate_screen_rect(new_size);

        let mut egui_input = self.egui_input.borrow_mut();

        egui_input.screen_rect = Some(screen_rect);

        let viewport_info = egui_input.viewports.get_mut(&self.viewport_id).unwrap();
        viewport_info.native_pixels_per_point = Some(new_size.scale_factor as f32);
        viewport_info.inner_rect = Some(screen_rect);

        // Schedule to repaint on the next frame.
        self.repaint_after.set(Some(Instant::now()));
    }

    #[allow(unused_variables)]
    fn on_event(&self, event: Event) -> EventStatus {
        let mut return_status = EventStatus::Captured;

        // Parent/embedded windows do not always gain keyboard focus
        // Automatically on click. Request focus explicitly before forwarding the event.
        if matches!(
            event,
            Event::Mouse(baseview::MouseEvent::ButtonPressed { .. })
        ) && !self.window.has_focus()
        {
            self.window.focus();
        }

        match &event {
            baseview::Event::Mouse(event) => match event {
                baseview::MouseEvent::CursorMoved {
                    position,
                    modifiers,
                } => {
                    self.update_modifiers(modifiers);

                    let pos = pos2(position.x as f32, position.y as f32);
                    self.pointer_pos_in_points.set(Some(pos));
                    self.egui_input
                        .borrow_mut()
                        .events
                        .push(egui::Event::PointerMoved(pos));
                }
                baseview::MouseEvent::ButtonPressed { button, modifiers } => {
                    self.update_modifiers(modifiers);

                    if let Some(pos) = self.pointer_pos_in_points.get()
                        && let Some(button) = crate::translate::translate_mouse_button(*button)
                    {
                        let mut egui_input = self.egui_input.borrow_mut();
                        let modifiers = egui_input.modifiers;
                        egui_input.events.push(egui::Event::PointerButton {
                            pos,
                            button,
                            pressed: true,
                            modifiers,
                        });
                    }
                }
                baseview::MouseEvent::ButtonReleased { button, modifiers } => {
                    self.update_modifiers(modifiers);

                    if let Some(pos) = self.pointer_pos_in_points.get()
                        && let Some(button) = crate::translate::translate_mouse_button(*button)
                    {
                        let mut egui_input = self.egui_input.borrow_mut();
                        let modifiers = egui_input.modifiers;
                        egui_input.events.push(egui::Event::PointerButton {
                            pos,
                            button,
                            pressed: false,
                            modifiers,
                        });
                    }
                }
                baseview::MouseEvent::WheelScrolled {
                    delta: scroll_delta,
                    modifiers,
                } => {
                    self.update_modifiers(modifiers);

                    #[allow(unused_mut)]
                    let (unit, mut delta) = match scroll_delta {
                        baseview::ScrollDelta::Lines { x, y } => {
                            (egui::MouseWheelUnit::Line, egui::vec2(*x, *y))
                        }

                        baseview::ScrollDelta::Pixels { x, y } => (
                            egui::MouseWheelUnit::Point,
                            egui::vec2(*x, *y) * self.window.scale_factor() as f32,
                        ),
                    };

                    if cfg!(target_os = "macos") {
                        // This is still buggy in winit despite
                        // https://github.com/rust-windowing/winit/issues/1695 being closed
                        //
                        // TODO: See if this is an issue in baseview as well.
                        delta.x *= -1.0;
                    }

                    let mut egui_input = self.egui_input.borrow_mut();
                    let modifiers = egui_input.modifiers;
                    egui_input.events.push(egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                        phase: egui::TouchPhase::Move,
                    });
                }
                baseview::MouseEvent::CursorLeft => {
                    self.pointer_pos_in_points.set(None);
                    self.egui_input
                        .borrow_mut()
                        .events
                        .push(egui::Event::PointerGone);
                }
                _ => {}
            },
            baseview::Event::Keyboard(event) => {
                use keyboard_types::Code;

                let pressed = event.state == keyboard_types::KeyState::Down;
                let mut egui_input = self.egui_input.borrow_mut();

                match event.code {
                    Code::ShiftLeft | Code::ShiftRight => egui_input.modifiers.shift = pressed,
                    Code::ControlLeft | Code::ControlRight => {
                        egui_input.modifiers.ctrl = pressed;

                        #[cfg(not(target_os = "macos"))]
                        {
                            egui_input.modifiers.command = pressed;
                        }
                    }
                    Code::AltLeft | Code::AltRight => egui_input.modifiers.alt = pressed,
                    Code::MetaLeft | Code::MetaRight => {
                        #[cfg(target_os = "macos")]
                        {
                            egui_input.modifiers.mac_cmd = pressed;
                            egui_input.modifiers.command = pressed;
                        }
                        // prevent `rustfmt` from breaking this
                    }
                    _ => (),
                }

                if let Some(key) = crate::translate::translate_virtual_key(&event.key) {
                    let mut egui_input = self.egui_input.borrow_mut();
                    let modifiers = egui_input.modifiers;
                    egui_input.events.push(egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: event.repeat,
                        modifiers,
                    });
                }

                if pressed {
                    let mut egui_input = self.egui_input.borrow_mut();
                    // VirtualKeyCode::Paste etc in winit are broken/untrustworthy,
                    // so we detect these things manually:
                    //
                    // TODO: See if this is an issue in baseview as well.
                    if is_cut_command(egui_input.modifiers, event.code) {
                        egui_input.events.push(egui::Event::Cut);
                    } else if is_copy_command(egui_input.modifiers, event.code) {
                        egui_input.events.push(egui::Event::Copy);
                    } else if is_paste_command(egui_input.modifiers, event.code) {
                        if let Some(clipboard_ctx) = self.clipboard_ctx.borrow_mut().as_mut() {
                            match clipboard_ctx.get_contents() {
                                Ok(contents) => egui_input.events.push(egui::Event::Text(contents)),
                                Err(err) => {
                                    error!("Paste error: {}", err);
                                }
                            }
                        }
                    } else if let keyboard_types::Key::Character(written) = &event.key
                        && !egui_input.modifiers.ctrl
                        && !egui_input.modifiers.command
                    {
                        egui_input.events.push(egui::Event::Text(written.clone()));
                    }
                }

                match &*self.key_capture.borrow() {
                    KeyCapture::CaptureAll => {}
                    KeyCapture::IgnoreAll => return_status = EventStatus::Ignored,
                    KeyCapture::CaptureKeys(keys) => {
                        if !keys.contains(&event.key) {
                            return_status = EventStatus::Ignored
                        }
                    }
                    KeyCapture::IgnoreKeys(keys) => {
                        if keys.contains(&event.key) {
                            return_status = EventStatus::Ignored
                        }
                    }
                }
            }
            baseview::Event::Window(event) => match event {
                baseview::WindowEvent::Focused => {
                    let mut egui_input = self.egui_input.borrow_mut();
                    egui_input.events.push(egui::Event::WindowFocused(true));
                    egui_input
                        .viewports
                        .get_mut(&self.viewport_id)
                        .unwrap()
                        .focused = Some(true);
                }
                baseview::WindowEvent::Unfocused => {
                    let mut egui_input = self.egui_input.borrow_mut();
                    egui_input.events.push(egui::Event::WindowFocused(false));
                    egui_input
                        .viewports
                        .get_mut(&self.viewport_id)
                        .unwrap()
                        .focused = Some(false);
                }
                baseview::WindowEvent::WillClose => {}
                _ => {}
            },
            _ => {}
        }

        // For keyboard events, also check if egui actually wants keyboard input
        // This allows DAW shortcuts (spacebar, etc.) to pass through when no text field is focused
        match &event {
            baseview::Event::Keyboard(_) => {
                let egui_ctx = self.egui_ctx.borrow();
                if return_status == EventStatus::Captured && !egui_ctx.egui_wants_keyboard_input() {
                    EventStatus::Ignored
                } else {
                    return_status
                }
            }
            baseview::Event::Mouse(_) => {
                let egui_ctx = self.egui_ctx.borrow();
                if egui_ctx.egui_is_using_pointer() || egui_ctx.egui_wants_pointer_input() {
                    EventStatus::Captured
                } else {
                    EventStatus::Ignored
                }
            }
            baseview::Event::Window(_) => EventStatus::Captured,
            _ => EventStatus::Ignored,
        }
    }
}

fn is_cut_command(modifiers: egui::Modifiers, keycode: keyboard_types::Code) -> bool {
    (modifiers.command && keycode == keyboard_types::Code::KeyX)
        || (cfg!(target_os = "windows")
            && modifiers.shift
            && keycode == keyboard_types::Code::Delete)
}

fn is_copy_command(modifiers: egui::Modifiers, keycode: keyboard_types::Code) -> bool {
    (modifiers.command && keycode == keyboard_types::Code::KeyC)
        || (cfg!(target_os = "windows")
            && modifiers.ctrl
            && keycode == keyboard_types::Code::Insert)
}

fn is_paste_command(modifiers: egui::Modifiers, keycode: keyboard_types::Code) -> bool {
    (modifiers.command && keycode == keyboard_types::Code::KeyV)
        || (cfg!(target_os = "windows")
            && modifiers.shift
            && keycode == keyboard_types::Code::Insert)
}

/// Calculate screen rectangle in logical size.
fn calculate_screen_rect(size: WindowSize) -> Rect {
    Rect::from_min_size(
        Pos2::new(0f32, 0f32),
        vec2(size.logical.width as f32, size.logical.height as f32),
    )
}
