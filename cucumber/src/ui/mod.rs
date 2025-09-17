use std::{
    collections::{BTreeMap, VecDeque},
    mem,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
};

use eframe::{
    egui::{self, CentralPanel, Context, Frame, Theme},
    App,
};
use egui_file_dialog::FileDialog;
use krakatau2::zip;
use re_ui::notifications::NotificationUi;
use tracing::error;

use crate::{
    extract_general_goodies,
    types::{CucumberBitwigTheme, NamedColor, StageProgress, ThemeOperation, ThemeProcessingEvent},
    ui::{
        central_panel::central_panel,
        command_palette::CommandPalette,
        commands::{
            command_channel, CommandReceiver, CommandSender, CucumberCommand, CucumberCommandSender,
        },
        drag_n_drop::handle_dropping_files,
        left_panel::left_panel,
        notifier::UiNotifier,
        right_panel::right_panel,
        status_bar::{status_bar, Progress, StatusBar},
    },
    writing::write_theme_to_jar,
    GeneralGoodies,
};

mod central_panel;
mod command_palette;
mod commands;
mod drag_n_drop;
mod left_panel;
pub mod notifier;
mod right_panel;
mod status_bar;
mod top_bar;

pub struct MyApp {
    jar_in: PathBuf,
    jar_out: Option<PathBuf>,
    log: VecDeque<String>,
    theme: Option<CucumberBitwigTheme>,
    general_goodies: Option<GeneralGoodies>,
    selected_color: Option<String>,
    filter: String,
    file_dialog: FileDialog,
    changed_colors: BTreeMap<String, NamedColor>,
    event_rx: Receiver<Event>,
    notifier: UiNotifier,
    panels_state: PanelsState,
    command_sender: CommandSender,
    command_receiver: CommandReceiver,
    status_bar: StatusBar,
    command_palette: CommandPalette,
    notifications: NotificationUi,
}

struct PanelsState {
    show_left_panel: bool,
    show_right_panel: bool,
    show_bottom_panel: bool,
}

pub enum Event {
    JarParsed { loaded_jar: LoadedJar },
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

pub struct LoadedJar {
    theme: CucumberBitwigTheme,
    general_goodies: GeneralGoodies,
}

impl LoadedJar {
    fn from_jar(
        zip: &mut zip::ZipArchive<std::fs::File>,
        report_progress: impl FnMut(ThemeProcessingEvent),
    ) -> anyhow::Result<Self> {
        let general_goodies = extract_general_goodies(zip, report_progress).unwrap();
        let theme = CucumberBitwigTheme::from_general_goodies(&general_goodies);

        Ok(LoadedJar {
            theme,
            general_goodies,
        })
    }
}

fn load_jar(
    jar_in: impl AsRef<Path>,
    report_progress: impl FnMut(ThemeProcessingEvent),
) -> anyhow::Result<LoadedJar> {
    let file = std::fs::File::open(jar_in)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let loaded_jar = LoadedJar::from_jar(&mut zip, report_progress)?;
    Ok(loaded_jar)
}

impl MyApp {
    pub fn new(
        ctx: Context,
        jar_in: Option<PathBuf>,
        jar_out: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        re_ui::apply_style_and_install_loaders(&ctx);

        let jar_in = jar_in.unwrap();
        let log = VecDeque::with_capacity(256);

        let (event_tx, event_rx) = mpsc::channel();

        let notifier = UiNotifier::new(ctx.clone(), event_tx);

        {
            let jar_in = jar_in.clone();

            let notifier = notifier.clone();
            std::thread::spawn(move || {
                let theme = load_jar(jar_in, |event| {
                    notifier.notify(Event::Progress(ProgressEvent::ThemeOperation {
                        event,
                        operation: ThemeOperation::LoadingFromJar,
                    }));
                })
                .unwrap();

                notifier.notify(Event::JarParsed { loaded_jar: theme });
            });
        }

        let file_dialog = FileDialog::new().add_file_filter(
            "JSON",
            Arc::new(|path| path.to_string_lossy().to_lowercase().ends_with("json")),
        );

        let (command_sender, command_receiver) = command_channel(ctx.clone());

        Ok(Self {
            jar_in,
            jar_out,
            log,
            theme: None,
            general_goodies: None,
            filter: String::new(),
            selected_color: None,
            file_dialog,
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
            command_palette: CommandPalette::default(),
            notifications: NotificationUi::new(ctx),
        })
    }

    fn handle_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                Event::JarParsed {
                    loaded_jar:
                        LoadedJar {
                            theme,
                            general_goodies,
                        },
                } => {
                    if self.selected_color.is_none() {
                        for color in ["On", "Accent (default)"] {
                            if theme.named_colors.contains_key(color) {
                                self.selected_color = Some(color.into());
                                break;
                            }
                        }
                    }

                    self.theme = Some(theme);
                    self.general_goodies = Some(general_goodies);
                }
                Event::Progress(progress_event) => match progress_event {
                    ProgressEvent::ThemeOperation { event, operation } => match event.progress {
                        StageProgress::Unknown => {
                            self.status_bar.progress = Some(Progress::new(
                                operation.as_str(),
                                event.stage.as_str(),
                                None,
                            ));
                        }
                        StageProgress::Percentage(value) => {
                            self.status_bar.progress = Some(Progress::new(
                                operation.as_str(),
                                event.stage.as_str(),
                                Some(value),
                            ));
                        }
                        StageProgress::Done => {
                            self.status_bar.progress = None;
                            self.notifications.success(format!(
                                "{}: {}",
                                operation.as_str(),
                                event.stage.as_str()
                            ));
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

    fn handle_commands(&mut self, ctx: &eframe::egui::Context) {
        while let Some(command) = self.command_receiver.recv() {
            match command {
                CucumberCommand::Quit => {
                    std::process::exit(0);
                }
                CucumberCommand::ToggleTheme => {
                    ctx.set_theme(match ctx.theme() {
                        Theme::Light => Theme::Dark,
                        Theme::Dark => Theme::Light,
                    });
                }
                CucumberCommand::ToggleCommandPalette => {
                    self.command_palette.toggle();
                }
                CucumberCommand::ZoomIn => {
                    let mut zoom_factor = ctx.zoom_factor();
                    zoom_factor += 0.1;
                    ctx.set_zoom_factor(zoom_factor);
                }
                CucumberCommand::ZoomOut => {
                    let mut zoom_factor = ctx.zoom_factor();
                    zoom_factor -= 0.1;
                    ctx.set_zoom_factor(zoom_factor);
                }
                CucumberCommand::ZoomReset => {
                    ctx.set_zoom_factor(1.0);
                }
                CucumberCommand::LoadJar(pathbuf) => {
                    if let Some(pathbuf) = pathbuf {
                        self.jar_in = pathbuf.clone();
                        let notifier = self.notifier.clone();
                        let jar_in = pathbuf;
                        std::thread::spawn(move || {
                            let loaded_jar = load_jar(jar_in, |event| {
                                notifier.notify(Event::Progress(ProgressEvent::ThemeOperation {
                                    event,
                                    operation: ThemeOperation::LoadingFromJar,
                                }));
                            })
                            .unwrap();

                            notifier.notify(Event::JarParsed { loaded_jar });
                        });
                    } else {
                        todo!("Show file picker")
                    }
                }
                CucumberCommand::SaveJar => {
                    if !self.is_dirty() {
                        continue;
                    }
                    let jar_in = self.jar_in.clone();
                    let jar_out = self.jar_out.clone().unwrap_or(jar_in.clone());
                    let notifier = self.notifier.clone();
                    let mut changed_colors = BTreeMap::new();
                    mem::swap(&mut changed_colors, &mut self.changed_colors);
                    std::thread::spawn(move || {
                        if let Err(err) =
                            write_theme_to_jar(jar_in, jar_out, changed_colors, |event| {
                                notifier.notify(Event::Progress(ProgressEvent::ThemeOperation {
                                    event,
                                    operation: ThemeOperation::WritingToJar,
                                }));
                            })
                        {
                            error!("Failed to save JAR: {}", err);
                        };
                    });
                }
            }
        }
    }

    /// Means we have unsaved changes.
    fn is_dirty(&self) -> bool {
        !self.changed_colors.is_empty()
    }
}

impl App for MyApp {
    fn update(&mut self, egui_ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        if let Some(cmd) = CucumberCommand::listen_for_kb_shortcut(egui_ctx) {
            self.command_sender.send_ui(cmd);
        }

        self.handle_events();
        self.handle_commands(egui_ctx);

        let frame = Frame::central_panel(&egui_ctx.style());

        top_bar::top_bar(
            &self.command_sender,
            &mut self.panels_state,
            egui_ctx,
            &self.status_bar.progress,
            self.changed_colors.len(),
        );

        egui::TopBottomPanel::bottom("bottom_panel").show_animated(
            egui_ctx,
            self.panels_state.show_bottom_panel,
            |ui| {
                status_bar(ui, &self.status_bar);
            },
        );

        egui::SidePanel::left("left_panel")
            .min_width(270.0)
            .frame(egui::Frame {
                fill: egui_ctx.style().visuals.panel_fill,
                ..Default::default()
            })
            .show_animated(egui_ctx, self.panels_state.show_left_panel, |ui| {
                left_panel(
                    ui,
                    self.theme.as_mut(),
                    &mut self.selected_color,
                    &mut self.changed_colors,
                );
            });

        egui::SidePanel::right("right_panel")
            .min_width(270.0)
            .frame(egui::Frame {
                fill: egui_ctx.style().visuals.panel_fill,
                ..Default::default()
            })
            .show_animated(egui_ctx, self.panels_state.show_right_panel, |ui| {
                right_panel(ui, &self.general_goodies);
            });

        CentralPanel::default().frame(frame).show(egui_ctx, |ui| {
            central_panel(ui);
        });

        if let Some(command) = self.command_palette.show(egui_ctx) {
            self.command_sender.send_ui(command);
        }

        // TODO: enable this and move toasts to the right bottom
        // self.notifications.show_toasts(egui_ctx);

        handle_dropping_files(egui_ctx, &self.command_sender);
    }
}
