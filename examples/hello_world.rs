use baseview::dpi::{LogicalSize, Size};
use egui::{CentralPanel, Context, FullOutput, Ui, ViewportOutput};
use egui_baseview::{EguiWindow, EguiWindowSettings, ExtraOutputCommands};

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
        |_egui_ctx: &Context, _commands: &mut ExtraOutputCommands, _state: &mut ()| {},
        |_output: &FullOutput, _viewport_output: &ViewportOutput, _state: &mut ()| {},
        |ui: &mut Ui, _commands: &mut ExtraOutputCommands, _state: &mut ()| {
            CentralPanel::default().show(ui, |ui| {
                ui.label("Hello World!");
            });
        },
    );
}
