mod renderer;
mod translate;
mod window;

pub use baseview;
pub use keyboard_types::Key;
pub use renderer::GraphicsConfig;
pub use window::{EguiWindow, EguiWindowSettings, Frame, KeyCapture};

/// Implement this trait to run an app with egui-baseview.
pub trait App: Send + 'static {
    /// Called once before the first frame. Setup code such as `egui_ctx.set_fonts()`
    /// can be done here.
    ///
    /// If an error is returned, then the window will be closed.
    fn build(
        &mut self,
        egui_ctx: &egui::Context,
        frame: &mut Frame,
    ) -> Result<(), baseview::HandlerError> {
        let _ = egui_ctx;
        let _ = frame;
        Ok(())
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame);

    /// Called after each `ui` call. This can be used to read egui's output commands
    /// to perform plugin-related actions, i.e. asking the host to resize the window
    /// if a command to resize the window is present.
    fn output(&mut self, output: &egui::FullOutput, viewport_output: &egui::ViewportOutput) {
        let _ = output;
        let _ = viewport_output;
    }
}
