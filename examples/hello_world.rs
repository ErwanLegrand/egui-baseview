use baseview::dpi::{LogicalSize, Size};
use egui::{CentralPanel, Context, Ui};
use egui_baseview::{EguiWindow, EguiWindowSettings, Queue};

fn main() {
    let state = ();

    EguiWindow::open_blocking(
        EguiWindowSettings::new()
            .with_tile("egui-baseview hello world")
            .with_size(Size::Logical(LogicalSize {
                width: 300.0,
                height: 110.0,
            })),
        state,
        |_egui_ctx: &Context, _queue: &mut Queue, _state: &mut ()| {},
        |ui: &mut Ui, _queue: &mut Queue, _state: &mut ()| {
            CentralPanel::default().show(ui, |ui| {
                ui.label("Hello World!");
            });
        },
    );
}
