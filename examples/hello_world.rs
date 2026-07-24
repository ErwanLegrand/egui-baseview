use egui::CentralPanel;
use egui_baseview::{
    EguiWindow, EguiWindowSettings,
    baseview::dpi::{LogicalSize, Size},
};

fn main() {
    EguiWindow::create(
        EguiWindowSettings::new()
            .with_title("egui-baseview hello world")
            .with_size(Size::Logical(LogicalSize {
                width: 300.0,
                height: 110.0,
            })),
        MyApp,
    )
    .unwrap()
    .run_until_closed()
    .unwrap();
}

struct MyApp;

impl egui_baseview::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut egui_baseview::Frame) {
        CentralPanel::default().show(ui, |ui| {
            ui.label("Hello World!");
        });
    }
}
