use baseview::dpi::{LogicalSize, Size};
use egui::{CentralPanel, Context, FullOutput, Ui, ViewportOutput};
use egui_baseview::{EguiWindow, EguiWindowSettings, ExtraOutputCommands};

fn main() {
    let state = State::new();

    EguiWindow::create(
        EguiWindowSettings::new()
            .with_title("egui-baseview simple demo")
            .with_size(Size::Logical(LogicalSize {
                width: 400.0,
                height: 200.0,
            })),
        state,
        // Called once before the first frame. Allows you to do setup code and to
        // call `ctx.set_fonts()`. Optional.
        |_egui_ctx: &Context, _commands: &mut ExtraOutputCommands, _state: &mut State| {},
        // Called after each `update`. Can be used to read egui's output commands to
        // perform actions, i.e. asking the host to resize the window if a command to
        // resize the window is present. Optional.
        |_output: &FullOutput, _viewport_output: &ViewportOutput, _state: &mut State| {},
        // Called before each frame. Here you should update the state of your
        // application and build the UI.
        |ui: &mut Ui, _commands: &mut ExtraOutputCommands, state: &mut State| {
            CentralPanel::default().show(ui, |ui| {
                ui.heading("My Egui Application");
                ui.horizontal(|ui| {
                    ui.label("Your name: ");
                    ui.text_edit_singleline(&mut state.name);
                });
                ui.add(egui::Slider::new(&mut state.age, 0..=120).text("age"));
                if ui.button("Click each year").clicked() {
                    state.age += 1;
                }
                ui.label(format!("Hello '{}', age {}", state.name, state.age));
                if ui.button("close window").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                ui.hyperlink_to("free crouton", "https://crouton.net");
            });
        },
    )
    .run_until_closed()
    .unwrap();
}

struct State {
    pub name: String,
    pub age: u32,
}

impl State {
    pub fn new() -> State {
        State {
            name: String::from(""),
            age: 30,
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        println!("Window is closing!");
    }
}
