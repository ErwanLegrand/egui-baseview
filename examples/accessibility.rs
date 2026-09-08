use egui::{CentralPanel, Slider};
use egui_baseview::{
    EguiWindow, EguiWindowSettings, Frame, RepaintNotifier,
    baseview::{HandlerError, dpi::LogicalSize},
};

fn main() {
    let repaint_notifier = RepaintNotifier::new();

    EguiWindow::create(
        EguiWindowSettings::new()
            .with_title("egui-baseview accessibility demo")
            .with_size(LogicalSize {
                width: 460.0,
                height: 380.0,
            })
            .with_zoom_factor(1.0)
            .with_repaint_notifier(repaint_notifier),
        AudioPluginApp::default(),
    )
    .unwrap()
    .run_until_closed()
    .unwrap();
}

enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

struct AudioPluginApp {
    track_name: String,
    playback_state: PlaybackState,
    gain: f32,
    frequency: f32,
    muted: bool,
}

impl Default for AudioPluginApp {
    fn default() -> Self {
        Self {
            track_name: "Master Track".to_string(),
            playback_state: PlaybackState::Stopped,
            gain: 0.75,
            frequency: 440.0,
            muted: false,
        }
    }
}

impl egui_baseview::App for AudioPluginApp {
    fn build(&mut self, _egui_ctx: egui::Context, _frame: &mut Frame) -> Result<(), HandlerError> {
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        CentralPanel::default().show(ui, |ui| {
            ui.heading("Accessible Audio Plugin");
            ui.add_space(8.0);

            ui.group(|ui| {
                ui.label("Track Settings");
                ui.horizontal(|ui| {
                    ui.label("Track Name:");
                    ui.text_edit_singleline(&mut self.track_name);
                });
            });

            ui.add_space(8.0);

            ui.group(|ui| {
                ui.label("Playback Controls");
                ui.horizontal(|ui| {
                    if ui.button("Play").clicked() {
                        self.playback_state = PlaybackState::Playing;
                    }
                    if ui.button("Pause").clicked() {
                        self.playback_state = PlaybackState::Paused;
                    }
                    if ui.button("Reset").clicked() {
                        self.playback_state = PlaybackState::Stopped;
                    }
                });

                let state_text = match self.playback_state {
                    PlaybackState::Stopped => "Stopped",
                    PlaybackState::Playing => "Playing",
                    PlaybackState::Paused => "Paused",
                };
                ui.label(format!("Current State: {}", state_text));
            });

            ui.add_space(8.0);

            ui.group(|ui| {
                ui.label("Audio Parameters");

                ui.horizontal(|ui| {
                    ui.label("Gain:");
                    ui.add(
                        Slider::new(&mut self.gain, 0.0..=1.0)
                            .text("level")
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Frequency:");
                    ui.add(
                        Slider::new(&mut self.frequency, 20.0..=20000.0)
                            .text("Hz")
                            .logarithmic(true)
                    );
                });

                ui.checkbox(&mut self.muted, "Mute");
            });
        });
    }
}
