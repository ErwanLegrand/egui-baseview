use baseview::WindowOpenOptions;
use baseview::dpi::LogicalSize;
use egui::{CentralPanel, Context, Ui};
use egui_baseview::{EguiWindow, GraphicsConfig, Queue};

fn main() {
    let settings = WindowOpenOptions::new()
        .with_title("egui-baseview hello world")
        .with_size(LogicalSize::new(300.0, 110.0));

    let state = ();

    EguiWindow::open_blocking(
        settings,
        GraphicsConfig::default(),
        state,
        |_egui_ctx: &Context, _queue: &mut Queue, _state: &mut ()| {},
        |ui: &mut Ui, _queue: &mut Queue, _state: &mut ()| {
            CentralPanel::default().show_inside(ui, |ui| {
                ui.label("Hello World!");
            });
        },
    );
}
