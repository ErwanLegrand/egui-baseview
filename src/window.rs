use std::cell::{Cell, RefCell};
use std::time::Instant;

use baseview::dpi::{LogicalPosition, LogicalSize, Size};
use baseview::{
    Event, EventStatus, HandlerError, ParentWindowHandle, Window, WindowContext, WindowHandler,
    WindowSettings, WindowSize,
};
use copypasta::ClipboardProvider;
use egui::{Pos2, Rect, Rgba, ViewportCommand, pos2, vec2};
use keyboard_types::Modifiers;
use raw_window_handle::HasWindowHandle;

use crate::App;
use crate::{GraphicsConfig, renderer::Renderer};

#[cfg(all(feature = "log", not(feature = "tracing")))]
use log::{error, warn};
#[cfg(feature = "tracing")]
use tracing::{error, warn};

#[derive(Debug, Clone)]
pub struct EguiWindowSettings {
    pub title: String,

    /// The size of the window
    pub size: Size,

    pub graphics: GraphicsConfig,

    pub parent: Option<ParentWindowHandle>,
}

impl EguiWindowSettings {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    #[inline]
    pub fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }

    #[inline]
    pub fn with_parent<'a, P: HasWindowHandle + 'a>(
        mut self,
        parent: impl Into<Option<&'a P>>,
    ) -> Self {
        let Some(parent) = parent.into() else {
            return self;
        };

        self.parent = Some(ParentWindowHandle::from_window(parent));
        self
    }

    #[inline]
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
            graphics: GraphicsConfig::default(),
            parent: None,
        }
    }
}

/// Represents the surroundings of your app.
pub struct Frame {
    clear_color: Rgba,
    key_capture: KeyCapture,
    renderer: Renderer,
    window: WindowContext,
}

impl Frame {
    /// Set the clear color of the renderer.
    pub fn clear_color(&mut self, clear_color: Rgba) {
        self.clear_color = clear_color;
    }

    /// Set how to handle capturing key events from the host.
    pub fn set_key_capture(&mut self, key_capture: KeyCapture) {
        self.key_capture = key_capture;
    }

    /// Access the internal baseview window.
    pub fn baseview_window(&self) -> &WindowContext {
        &self.window
    }

    /// A reference to the underlying
    /// [glow](https://docs.rs/glow/0.17.0/x86_64-unknown-linux-gnu/glow/index.html)
    /// (OpenGL) context.
    ///
    /// This can be used, for instance, to:
    ///
    /// * Render things to offscreen buffers.
    /// * Read the pixel buffer from the previous frame (glow::Context::read_pixels).
    /// * Render things behind the egui windows.
    ///
    /// Note that all egui painting is deferred to after the call to App::ui
    /// (egui only collects egui::Shapes and then egui-baseview paints them all in
    /// one go later on).
    #[cfg(feature = "opengl")]
    pub fn gl(&self) -> &std::sync::Arc<egui_glow::glow::Context> {
        &self.renderer.glow_context
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

struct EguiWindowInner<A: App> {
    user_app: A,
    egui_ctx: egui::Context,
    clipboard_ctx: Option<copypasta::ClipboardContext>,
    frame: Frame,
}

/// Handles an egui-baseview application
pub struct EguiWindow<A: App> {
    inner: RefCell<EguiWindowInner<A>>,
    egui_input: RefCell<egui::RawInput>,
    viewport_id: egui::ViewportId,
    start_time: Instant,
    scale_factor: Cell<f64>,
    pointer_logical_pos: Cell<Option<egui::Pos2>>,
    current_cursor_icon: Cell<baseview::MouseCursor>,
    repaint_after: Cell<Option<Instant>>,

    pub window: WindowContext,
}

impl<A: App> EguiWindow<A> {
    fn new(
        window: WindowContext,
        title: String,
        graphics_config: GraphicsConfig,
        mut user_app: A,
    ) -> Result<EguiWindow<A>, HandlerError> {
        let renderer = Renderer::new(window.clone(), graphics_config)?;
        let egui_ctx = egui::Context::default();

        let size = window.size();

        let screen_rect = logical_screen_rect(size);
        let scale_factor = size.scale_factor;

        let viewport_info = egui::ViewportInfo {
            parent: None,
            title: Some(title),
            native_pixels_per_point: Some(scale_factor as f32),
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

        let mut frame = Frame {
            clear_color: Rgba::BLACK,
            key_capture: Default::default(),
            renderer,
            window: window.clone(),
        };

        user_app.build(&egui_ctx, &mut frame)?;

        let clipboard_ctx = match copypasta::ClipboardContext::new() {
            Ok(clipboard_ctx) => Some(clipboard_ctx),
            Err(e) => {
                #[cfg(any(feature = "tracing", feature = "log"))]
                error!("Failed to initialize clipboard: {}", e);

                #[cfg(not(any(feature = "tracing", feature = "log")))]
                let _ = e;

                None
            }
        };

        let start_time = Instant::now();

        Ok(Self {
            inner: RefCell::new(EguiWindowInner {
                user_app,
                egui_ctx,
                clipboard_ctx,
                frame,
            }),
            viewport_id,
            start_time,
            egui_input: egui_input.into(),
            pointer_logical_pos: None.into(),
            current_cursor_icon: baseview::MouseCursor::Default.into(),
            scale_factor: scale_factor.into(),
            repaint_after: Some(start_time).into(),
            window,
        })
    }

    /// Open a new window.
    ///
    /// * `settings` - The settings of the window.
    /// * `app` - The application to run.
    pub fn create(settings: EguiWindowSettings, app: A) -> Result<Window, baseview::Error> {
        let mut options = WindowSettings::new()
            .with_title(settings.title.clone())
            .with_size(settings.size);

        options.parent = settings.parent;

        #[cfg(feature = "opengl")]
        let options = { options.with_gl_config(Some(settings.graphics.gl_config.clone())) };

        Window::create(options, move |window| {
            EguiWindow::new(window, settings.title, settings.graphics, app)
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

impl<A: App> WindowHandler for EguiWindow<A> {
    fn on_frame(&self) -> Result<(), HandlerError> {
        let egui_input = {
            let mut egui_input = self.egui_input.borrow_mut();
            egui_input.time = Some(self.start_time.elapsed().as_secs_f64());
            egui_input.screen_rect = Some(logical_screen_rect(self.window.size()));
            egui_input.take()
        };

        let mut full_output = {
            let mut inner = self.inner.borrow_mut();
            let EguiWindowInner {
                user_app,
                egui_ctx,
                clipboard_ctx: _,
                frame,
            } = &mut *inner;

            let output = egui_ctx.run_ui(egui_input, |ui| user_app.ui(ui, frame));

            if let Some(viewport_output) = output.viewport_output.get(&self.viewport_id) {
                user_app.output(&output, viewport_output);
            }

            output
        };

        let Some(viewport_output) = full_output.viewport_output.get(&self.viewport_id) else {
            // The main window was closed by egui.
            self.window.request_close();
            return Ok(());
        };

        for command in viewport_output.commands.iter() {
            match command {
                ViewportCommand::Close => {
                    self.window.request_close();
                }
                ViewportCommand::InnerSize(size) => self.window.resize(LogicalSize {
                    width: size.x.max(1.0),
                    height: size.y.max(1.0),
                })?,
                ViewportCommand::Focus => {
                    self.window.focus()?;
                }
                _ => {}
            }
        }

        {
            let mut inner = self.inner.borrow_mut();
            let EguiWindowInner {
                user_app: _,
                egui_ctx,
                clipboard_ctx,
                frame,
            } = &mut *inner;

            let now = Instant::now();
            let do_repaint_now = if let Some(t) = self.repaint_after.get() {
                now >= t || viewport_output.repaint_delay.is_zero()
            } else {
                viewport_output.repaint_delay.is_zero()
            };

            if do_repaint_now {
                let size = self.window.size();
                frame.renderer.render(
                    &self.window,
                    frame.clear_color,
                    size.physical,
                    size.scale_factor as f32,
                    egui_ctx,
                    &mut full_output,
                );

                self.repaint_after.set(None);
            } else if let Some(t) = now.checked_add(viewport_output.repaint_delay) {
                // Schedule to repaint after the requested time has elapsed.
                self.repaint_after.set(Some(t));
            }

            for command in full_output.platform_output.commands {
                match command {
                    egui::OutputCommand::CopyText(text) => {
                        if let Some(clipboard_ctx) = clipboard_ctx.as_mut()
                            && let Err(err) = clipboard_ctx.set_contents(text)
                        {
                            #[cfg(any(feature = "tracing", feature = "log"))]
                            error!("Copy/Cut error: {}", err);

                            #[cfg(not(any(feature = "tracing", feature = "log")))]
                            let _ = err;
                        }
                    }
                    egui::OutputCommand::CopyImage(_) => {
                        #[cfg(any(feature = "tracing", feature = "log"))]
                        warn!("Copying images is not supported in egui_baseview.");
                    }
                    egui::OutputCommand::OpenUrl(open_url) => {
                        if let Err(err) = open::that_detached(&open_url.url) {
                            #[cfg(any(feature = "tracing", feature = "log"))]
                            error!("Open error: {}", err);

                            #[cfg(not(any(feature = "tracing", feature = "log")))]
                            let _ = err;
                        }
                    }
                }
            }
        }

        let cursor_icon =
            crate::translate::translate_cursor_icon(full_output.platform_output.cursor_icon);
        if self.current_cursor_icon.get() != cursor_icon {
            self.current_cursor_icon.set(cursor_icon);

            self.window.set_mouse_cursor(cursor_icon)?;
        }

        // A temporary workaround for keyboard input not working sometimes.
        // See https://codeberg.org/RustAudio/egui-baseview/issues/20
        #[cfg(feature = "keyboard_focus_workaround")]
        {
            if !full_output.platform_output.events.is_empty()
                || full_output.platform_output.ime.is_some()
            {
                window.focus();
            }
        }

        Ok(())
    }

    fn resized(&self, new_size: WindowSize) -> Result<(), HandlerError> {
        let screen_rect = logical_screen_rect(new_size);

        let mut egui_input = self.egui_input.borrow_mut();

        egui_input.screen_rect = Some(screen_rect);

        let viewport_info = egui_input.viewports.get_mut(&self.viewport_id).unwrap();
        viewport_info.native_pixels_per_point = Some(new_size.scale_factor as f32);
        viewport_info.inner_rect = Some(screen_rect);

        // Schedule to repaint on the next frame.
        self.repaint_after.set(Some(Instant::now()));

        self.scale_factor.set(new_size.scale_factor);
        Ok(())
    }

    fn on_event(&self, event: Event) -> EventStatus {
        let mut return_status = EventStatus::Captured;

        // Parent/embedded windows do not always gain keyboard focus
        // Automatically on click. Request focus explicitly before forwarding the event.
        //
        // TODO: Check if this is still necessary.
        if matches!(
            event,
            baseview::Event::Mouse(baseview::MouseEvent::ButtonPressed { .. })
        ) && !self.window.has_focus()
        {
            self.window.focus().unwrap();
        }

        match &event {
            baseview::Event::Mouse(event) => match event {
                baseview::MouseEvent::CursorMoved {
                    position,
                    modifiers,
                } => {
                    self.update_modifiers(modifiers);

                    let logical_pos: LogicalPosition<f32> =
                        position.to_logical(self.scale_factor.get());
                    let pos = pos2(logical_pos.x, logical_pos.y);

                    self.pointer_logical_pos.set(Some(pos));
                    self.egui_input
                        .borrow_mut()
                        .events
                        .push(egui::Event::PointerMoved(pos));
                }
                baseview::MouseEvent::ButtonPressed { button, modifiers } => {
                    self.update_modifiers(modifiers);

                    if let Some(pos) = self.pointer_logical_pos.get()
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

                    if let Some(pos) = self.pointer_logical_pos.get()
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
                    self.pointer_logical_pos.set(None);
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
                    // VirtualKeyCode::Paste etc in winit are broken/untrustworthy,
                    // so we detect these things manually:
                    //
                    // TODO: See if this is an issue in baseview as well.
                    if is_cut_command(egui_input.modifiers, event.code) {
                        egui_input.events.push(egui::Event::Cut);
                    } else if is_copy_command(egui_input.modifiers, event.code) {
                        egui_input.events.push(egui::Event::Copy);
                    } else if is_paste_command(egui_input.modifiers, event.code) {
                        if let Some(clipboard_ctx) = self.inner.borrow_mut().clipboard_ctx.as_mut()
                        {
                            match clipboard_ctx.get_contents() {
                                Ok(contents) => egui_input.events.push(egui::Event::Text(contents)),
                                Err(err) => {
                                    #[cfg(any(feature = "tracing", feature = "log"))]
                                    error!("Paste error: {}", err);

                                    #[cfg(not(any(feature = "tracing", feature = "log")))]
                                    let _ = err;
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

                match &self.inner.borrow().frame.key_capture {
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
                let egui_ctx = &self.inner.borrow().egui_ctx;
                if return_status == EventStatus::Captured && !egui_ctx.egui_wants_keyboard_input() {
                    EventStatus::Ignored
                } else {
                    return_status
                }
            }
            baseview::Event::Mouse(_) => {
                let egui_ctx = &self.inner.borrow().egui_ctx;
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
fn logical_screen_rect(size: WindowSize) -> Rect {
    Rect::from_min_size(
        Pos2::new(0f32, 0f32),
        vec2(size.logical.width as f32, size.logical.height as f32),
    )
}
