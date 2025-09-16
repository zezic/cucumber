use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
};

use eframe::{
    egui::{self, CentralPanel, Context, Frame, Vec2},
    App,
};
use egui_file_dialog::FileDialog;
use krakatau2::zip;

use crate::{
    types::{CucumberBitwigTheme, NamedColor, StageProgress, ThemeOperation, ThemeProcessingEvent},
    ui::{
        central_panel::central_panel,
        commands::{command_channel, CommandReceiver, CommandSender},
        left_panel::left_panel,
        notifier::UiNotifier,
        status_bar::{status_bar, Progress, StatusBar},
    },
};

mod central_panel;
mod command_palette;
mod commands;
mod left_panel;
pub mod notifier;
mod right_panel;
mod status_bar;
mod top_bar;

pub struct MyApp {
    jar_in: String,
    jar_out: Option<String>,
    log: VecDeque<String>,
    theme: Option<CucumberBitwigTheme>,
    selected_color: Option<String>,
    filter: String,
    first_run: bool,
    file_dialog: FileDialog,
    last_mockup_size: Vec2,
    changed_colors: BTreeMap<String, NamedColor>,
    event_rx: Receiver<Event>,
    notifier: UiNotifier,
    panels_state: PanelsState,
    command_sender: CommandSender,
    command_receiver: CommandReceiver,
    status_bar: StatusBar,
}

struct PanelsState {
    show_left_panel: bool,
    show_right_panel: bool,
    show_bottom_panel: bool,
}

pub enum Event {
    JarParsed { theme: CucumberBitwigTheme },
    Progress(ProgressEvent),
}

#[derive(Debug)]
pub enum ProgressEvent {
    ThemeOperation {
        event: ThemeProcessingEvent,
        operation: ThemeOperation,
    },
    Text(String),
}

fn load_theme_from_jar(
    jar_in: String,
    report_progress: impl FnMut(ThemeProcessingEvent),
) -> anyhow::Result<CucumberBitwigTheme> {
    let file = std::fs::File::open(jar_in)?;
    let mut zip = zip::ZipArchive::new(file)?;
    Ok(CucumberBitwigTheme::from_jar(&mut zip, report_progress))
}

impl MyApp {
    pub fn new(
        ctx: Context,
        jar_in: Option<String>,
        jar_out: Option<String>,
    ) -> anyhow::Result<Self> {
        re_ui::apply_style_and_install_loaders(&ctx);

        let jar_in = jar_in.unwrap();
        let log = VecDeque::with_capacity(256);

        let (event_tx, event_rx) = mpsc::channel();

        let notifier = UiNotifier::new(ctx, event_tx);

        {
            let jar_in = jar_in.clone();

            let notifier = notifier.clone();
            std::thread::spawn(move || {
                let theme = load_theme_from_jar(jar_in, |event| {
                    notifier.notify(Event::Progress(ProgressEvent::ThemeOperation {
                        event,
                        operation: ThemeOperation::LoadingFromJar,
                    }));
                })
                .unwrap();

                let mut changed_colors = HashSet::new();
                for (name, _color) in &theme.named_colors {
                    changed_colors.insert(name.clone());
                }
                notifier.notify(Event::JarParsed { theme });
            });
        }

        let file_dialog = FileDialog::new().add_file_filter(
            "JSON",
            Arc::new(|path| path.to_string_lossy().to_lowercase().ends_with("json")),
        );

        let (command_sender, command_receiver) = command_channel();

        Ok(Self {
            jar_in,
            jar_out,
            log,
            theme: None,
            filter: String::new(),
            selected_color: None,
            first_run: true,
            file_dialog,
            last_mockup_size: Vec2::default(),
            changed_colors: BTreeMap::new(),
            event_rx,
            notifier,
            panels_state: PanelsState {
                show_left_panel: true,
                show_right_panel: true,
                show_bottom_panel: true,
            },
            command_sender,
            command_receiver,
            status_bar: StatusBar::default(),
        })
    }

    fn handle_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                Event::JarParsed { theme } => {
                    if self.selected_color.is_none() {
                        for color in ["On", "Accent (default)"] {
                            if theme.named_colors.contains_key(color) {
                                self.selected_color = Some(color.into());
                                break;
                            }
                        }
                    }

                    self.theme = Some(theme);
                }
                Event::Progress(progress_event) => match progress_event {
                    ProgressEvent::ThemeOperation {
                        event: theme_loading_event,
                        operation,
                    } => match theme_loading_event.progress {
                        StageProgress::Unknown => {
                            self.status_bar.progress = Some(Progress::new(
                                operation.as_str(),
                                theme_loading_event.stage.as_str(),
                                None,
                            ));
                        }
                        StageProgress::Percentage(value) => {
                            self.status_bar.progress = Some(Progress::new(
                                operation.as_str(),
                                theme_loading_event.stage.as_str(),
                                Some(value),
                            ));
                        }
                        StageProgress::Done => {
                            self.status_bar.progress = None;
                        }
                    },
                    ProgressEvent::Text(text_event) => {
                        if self.log.len() == self.log.capacity() {
                            self.log.pop_front();
                        }
                        self.log.push_back(text_event);
                    }
                },
            }
        }
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events();

        let frame = Frame::central_panel(&ctx.style());

        top_bar::top_bar(&self.command_sender, &mut self.panels_state, ctx);

        egui::TopBottomPanel::bottom("bottom_panel").show_animated(
            ctx,
            self.panels_state.show_bottom_panel,
            |ui| {
                status_bar(ui, &self.status_bar);
            },
        );

        egui::SidePanel::left("left_panel")
            .min_width(270.0)
            .frame(egui::Frame {
                fill: ctx.style().visuals.panel_fill,
                ..Default::default()
            })
            .show_animated(ctx, self.panels_state.show_left_panel, |ui| {
                left_panel(ui, self.theme.as_mut(), &mut self.selected_color);
            });

        CentralPanel::default().frame(frame).show(ctx, |ui| {
            central_panel(ui);
        });
    }
}
