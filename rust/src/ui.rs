use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

use chromakit_rs::generation::{
    GenerationError, GenerationProgress, GenerationReport, GenerationSettings, generate,
};
use eframe::egui;

enum UiEvent {
    Progress(GenerationProgress),
    Finished(Result<GenerationReport, GenerationError>),
}

fn sync_default_output(input_folder: &str, output_folder: &mut String, manually_selected: bool) {
    if !manually_selected {
        output_folder.clear();
        output_folder.push_str(input_folder);
    }
}

pub struct ChromaKitApp {
    input_folder: String,
    output_folder: String,
    output_folder_manually_selected: bool,
    note_index: usize,
    octave: i32,
    note_range: usize,
    gap_text: String,
    pitch_samples: bool,
    export_individual: bool,
    auto_level: bool,
    dark_mode: bool,
    running: bool,
    progress: u8,
    status: String,
    error_message: Option<String>,
    cancelled: Option<Arc<AtomicBool>>,
    events: Option<Receiver<UiEvent>>,
}

impl ChromaKitApp {
    pub fn new(creation: &eframe::CreationContext<'_>) -> Self {
        creation.egui_ctx.set_visuals(Self::visuals(false));
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let mut style = (*creation.egui_ctx.style_of(theme)).clone();
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 5.0);
            style.spacing.interact_size.y = 26.0;
            creation.egui_ctx.set_style_of(theme, style);
        }
        Self {
            input_folder: String::new(),
            output_folder: String::new(),
            output_folder_manually_selected: false,
            note_index: 0,
            octave: 2,
            note_range: 24,
            gap_text: "0.1".into(),
            pitch_samples: true,
            export_individual: true,
            auto_level: false,
            dark_mode: false,
            running: false,
            progress: 0,
            status: "Ready".into(),
            error_message: None,
            cancelled: None,
            events: None,
        }
    }

    fn browse_folder(current: &str) -> Option<PathBuf> {
        let current = PathBuf::from(current);
        let mut dialog = rfd::FileDialog::new();
        if current.is_dir() {
            dialog = dialog.set_directory(current);
        } else if let Some(parent) = current.parent().filter(|path| path.is_dir()) {
            dialog = dialog.set_directory(parent);
        }
        dialog.pick_folder()
    }

    fn visuals(dark_mode: bool) -> egui::Visuals {
        let mut visuals = if dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let radius = egui::CornerRadius::same(6);
        visuals.widgets.noninteractive.corner_radius = radius;
        visuals.widgets.inactive.corner_radius = radius;
        visuals.widgets.hovered.corner_radius = radius;
        visuals.widgets.active.corner_radius = radius;
        visuals.widgets.open.corner_radius = radius;
        visuals.window_corner_radius = egui::CornerRadius::same(8);
        visuals.menu_corner_radius = egui::CornerRadius::same(6);
        visuals
    }

    fn start_generation(&mut self, ctx: &egui::Context) {
        let input_folder = PathBuf::from(&self.input_folder);
        let output_folder = PathBuf::from(&self.output_folder);
        let gap_seconds = match self.gap_text.trim().parse::<f64>() {
            Ok(gap) if gap.is_finite() && gap >= 0.0 => gap,
            _ => {
                self.error_message =
                    Some("Gap must be a nonnegative number of seconds, such as 0.1".into());
                self.status = "Check the generation settings".into();
                return;
            }
        };
        if self.input_folder.trim().is_empty() || self.output_folder.trim().is_empty() {
            self.error_message =
                Some("Choose both an input sample folder and an output folder.".into());
            self.status = "Choose folders to continue".into();
            return;
        }

        let settings = GenerationSettings {
            input_folder,
            output_folder,
            start_midi: 12 + self.octave * 12 + self.note_index as i32,
            range: self.note_range,
            gap_seconds,
            pitch_samples: self.pitch_samples,
            export_individual: self.export_individual,
            auto_level: self.auto_level,
            worker_count: 0,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (sender, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        let spawn = thread::Builder::new()
            .name("chromakit-generation".into())
            .spawn(move || {
                let result = generate(&settings, &worker_cancelled, |event| {
                    let _ = sender.send(UiEvent::Progress(event));
                    repaint.request_repaint();
                });
                let _ = sender.send(UiEvent::Finished(result));
                repaint.request_repaint();
            });
        match spawn {
            Ok(_) => {
                self.cancelled = Some(cancelled);
                self.events = Some(receiver);
                self.running = true;
                self.progress = 0;
                self.status = "Preparing numbered WAV inputs…".into();
                self.error_message = None;
            }
            Err(error) => {
                self.error_message = Some(format!("Could not start generation: {error}"));
                self.status = "Generation failed".into();
            }
        }
    }

    fn poll_worker(&mut self) {
        let pending = self
            .events
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for event in pending {
            match event {
                UiEvent::Progress(event) => {
                    self.progress = event.percent;
                    self.status = event.message;
                }
                UiEvent::Finished(Ok(report)) => {
                    self.running = false;
                    self.progress = 100;
                    self.status = format!(
                        "Complete: {} notes written to {}",
                        report.note_count,
                        report.chromatic_path.display()
                    );
                    self.cancelled = None;
                    self.events = None;
                }
                UiEvent::Finished(Err(GenerationError::Cancelled)) => {
                    self.running = false;
                    self.status = "Generation cancelled; incomplete outputs removed".into();
                    self.cancelled = None;
                    self.events = None;
                }
                UiEvent::Finished(Err(error)) => {
                    self.running = false;
                    self.status = "Generation failed".into();
                    self.error_message = Some(error.to_string());
                    self.cancelled = None;
                    self.events = None;
                }
            }
        }
    }

    fn folder_row(ui: &mut egui::Ui, label: &str, path: &mut String) -> (bool, bool) {
        ui.label(label);
        let mut browse = false;
        let mut edited = false;
        ui.horizontal_wrapped(|ui| {
            let width = (ui.available_width() - 86.0).max(40.0);
            edited = ui
                .add_sized(
                    [width, 28.0],
                    egui::TextEdit::singleline(path).hint_text("Choose a folder"),
                )
                .changed();
            browse = ui.button("Browse").clicked();
        });
        (browse, edited)
    }
}

impl eframe::App for ChromaKitApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.set_visuals(Self::visuals(self.dark_mode));
        self.poll_worker();

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("ChromaKit").size(24.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(if self.dark_mode {
                                "Light mode"
                            } else {
                                "Dark mode"
                            })
                            .clicked()
                        {
                            self.dark_mode = !self.dark_mode;
                        }
                    });
                });
                ui.add_space(8.0);

                let (browse_input, input_edited) =
                    Self::folder_row(ui, "Input samples", &mut self.input_folder);
                if input_edited {
                    sync_default_output(
                        &self.input_folder,
                        &mut self.output_folder,
                        self.output_folder_manually_selected,
                    );
                }
                if browse_input && let Some(path) = Self::browse_folder(&self.input_folder) {
                    self.input_folder = path.to_string_lossy().into_owned();
                    sync_default_output(
                        &self.input_folder,
                        &mut self.output_folder,
                        self.output_folder_manually_selected,
                    );
                }
                ui.add_space(6.0);
                let (browse_output, output_edited) =
                    Self::folder_row(ui, "Output folder", &mut self.output_folder);
                if output_edited {
                    self.output_folder_manually_selected = true;
                }
                if browse_output && let Some(path) = Self::browse_folder(&self.output_folder) {
                    self.output_folder = path.to_string_lossy().into_owned();
                    self.output_folder_manually_selected = true;
                }
                ui.add_space(12.0);

                let remaining =
                    (132 - (12 + self.octave * 12 + self.note_index as i32)).max(1) as usize;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Starting Note");
                    egui::ComboBox::from_id_salt("starting-note")
                        .selected_text(NOTES[self.note_index])
                        .show_ui(ui, |ui| {
                            for (index, note) in NOTES.iter().enumerate() {
                                ui.selectable_value(&mut self.note_index, index, *note);
                            }
                        });
                    ui.add_space(4.0);
                    ui.label("Octave");
                    egui::ComboBox::from_id_salt("starting-octave")
                        .selected_text(self.octave.to_string())
                        .show_ui(ui, |ui| {
                            for octave in 0..=9 {
                                ui.selectable_value(&mut self.octave, octave, octave.to_string());
                            }
                        });
                    ui.add_space(4.0);
                    ui.label("Number of Notes");
                    ui.add(egui::DragValue::new(&mut self.note_range).range(1..=remaining));
                });
                let max_range =
                    (132 - (12 + self.octave * 12 + self.note_index as i32)).max(1) as usize;
                self.note_range = self.note_range.clamp(1, max_range);
                ui.add_space(4.0);

                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.pitch_samples, "Pitch samples");
                    ui.checkbox(&mut self.export_individual, "Export individual WAV files")
                        .on_hover_text("Creates one WAV per note alongside chromatic.wav.");
                });
                ui.add_space(2.0);
                egui::CollapsingHeader::new("Advanced options")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Gap (seconds)");
                            ui.add_sized(
                                [96.0, 24.0],
                                egui::TextEdit::singleline(&mut self.gap_text),
                            );
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.auto_level, "Fix volume drops");
                        });
                    });
                ui.add_space(8.0);

                let generate =
                    egui::Button::new(egui::RichText::new("Generate chromatic WAVs").strong());
                let generate_clicked = ui
                    .add_enabled_ui(!self.running, |ui| {
                        ui.add_sized([ui.available_width(), 40.0], generate)
                    })
                    .inner
                    .clicked();
                if generate_clicked {
                    self.start_generation(&ctx);
                }

                if self.running {
                    ui.add_space(4.0);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let width = (ui.available_width() - 132.0).max(32.0);
                        ui.add_sized(
                            [width, 22.0],
                            egui::ProgressBar::new(f32::from(self.progress.min(100)) / 100.0)
                                .fill(ui.visuals().widgets.inactive.weak_bg_fill),
                        );
                        ui.label(format!("{}%", self.progress.min(100)));
                        if ui.button("Cancel").clicked()
                            && let Some(cancelled) = &self.cancelled
                        {
                            cancelled.store(true, Ordering::Relaxed);
                            self.status = "Cancelling after the current notes finish…".into();
                        }
                    });
                    ui.label(&self.status);
                } else if self.status.starts_with("Complete:")
                    || self.status.starts_with("Generation cancelled")
                {
                    ui.add_space(6.0);
                    ui.label(&self.status);
                }
                if let Some(error) = &self.error_message {
                    ui.add_space(6.0);
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });
        });
        if self.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

const NOTES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
