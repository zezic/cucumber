use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs::File,
    io::{BufReader, Read},
    path::Path,
    str::FromStr,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, RwLock,
    },
};

use anyhow::anyhow;
use eframe::{
    egui::{self, ecolor::HexColor, Context, Frame, Layout, Margin, ScrollArea, Sense, Vec2},
    epaint::Hsva,
    App,
};
use egui_file_dialog::FileDialog;
use krakatau2::{
    file_output_util::Writer,
    lib::{classfile, ParserOptions},
    zip,
};
use preview::Preview;
use tracing::{debug, info};
use xml::EmitterConfig;
use xmltree::Element;

use crate::{
    exchange::BerikaiTheme,
    extract_general_goodies,
    patching::patch_class,
    reasm, replace_named_color,
    types::{AbsoluteColor, CompositingMode, CucumberBitwigTheme, NamedColor, ThemeLoadingEvent},
    ColorComponents,
};

mod preview;

pub struct MyApp {
    jar_in: String,
    jar_out: Option<String>,
    log: VecDeque<LogRecord>,
    theme: Option<CucumberBitwigTheme>,
    selected_color: Option<String>,
    filter: String,
    first_run: bool,
    file_dialog: FileDialog,
    last_mockup_size: Vec2,
    mockup: Vec<u8>,
    img_src: egui::ImageSource<'static>,
    changed_colors: BTreeMap<String, NamedColor>,
    preview: Preview,
    rx: Receiver<CommonEvent>,
    notifier: UiNotifier,
}

#[derive(Clone)]
struct UiNotifier {
    ctx: Context,
    tx: Sender<CommonEvent>,
}

impl UiNotifier {
    fn new(ctx: Context, tx: Sender<CommonEvent>) -> Self {
        Self { ctx, tx }
    }

    fn notify(&self, event: CommonEvent) {
        self.tx.send(event).unwrap();
    }
}

pub enum CommonEvent {
    JarParsed { theme: CucumberBitwigTheme },
    Log(LogRecord),
    UpdatedImage(Vec<u8>),
}

#[derive(Debug)]
enum LogRecord {
    ThemeLoading(ThemeLoadingEvent),
    ThemeWriting(ThemeWritingEvent),
    Text(String),
}

#[derive(Debug)]
pub enum ThemeWritingEvent {
    Done,
}

fn load_theme_from_jar(
    jar_in: String,
    report_progress: impl FnMut(ThemeLoadingEvent),
) -> anyhow::Result<CucumberBitwigTheme> {
    let file = std::fs::File::open(jar_in)?;
    let mut zip = zip::ZipArchive::new(file)?;
    Ok(CucumberBitwigTheme::from_jar(&mut zip, report_progress))
}

fn write_theme_to_jar(
    jar_in: String,
    jar_out: String,
    theme: CucumberBitwigTheme,
    mut report_progress: impl FnMut(ThemeWritingEvent),
) -> anyhow::Result<()> {
    let file = std::fs::File::open(jar_in)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let mut general_goodies = extract_general_goodies(&mut zip, |_| {})?;
    let mut patched_classes = HashMap::new();

    let mut file = zip.by_name(&general_goodies.init_class)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    let mut class = classfile::parse(
        &buffer,
        ParserOptions {
            no_short_code_attr: true,
        },
    )
    .map_err(|err| anyhow!("Parse: {:?}", err))?;
    patch_class(&mut class);
    let patched = reasm(file.name(), &class).unwrap();
    patched_classes.insert(file.name().to_string(), patched);
    drop(file);

    let named_colors_copy = general_goodies.named_colors.clone();
    for clr in named_colors_copy {
        let file_name_w_ext = format!("{}.class", clr.class_name);
        let buffer = match patched_classes.remove(&file_name_w_ext) {
            Some(patched) => patched,
            None => {
                let mut file = zip.by_name(&file_name_w_ext)?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                buffer
            }
        };

        let mut class = classfile::parse(
            &buffer,
            ParserOptions {
                no_short_code_attr: true,
            },
        )
        .map_err(|err| anyhow!("Parse: {:?}", err))?;

        if let Some(NamedColor::Absolute(repl)) = theme.named_colors.get(&clr.color_name) {
            let [r, g, b, a] = Hsva::new(repl.h, repl.s, repl.v, repl.a).to_srgba_unmultiplied();

            let new_value = match repl.compositing_mode {
                Some(CompositingMode::RelativeToBackground) => {
                    ColorComponents::Hsvf(repl.h, repl.s, repl.v)
                }
                _ => ColorComponents::Rgbai(r, g, b, a),
            };
            if replace_named_color(
                &mut class,
                &clr.color_name,
                new_value,
                &mut general_goodies.named_colors,
                &general_goodies.palette_color_methods,
                repl.compositing_mode.clone(),
            )
            .is_none()
            {
                debug!("failed to replace in {}", file_name_w_ext);
            }

            let new_buffer = reasm(&file_name_w_ext, &class)?;
            patched_classes.insert(file_name_w_ext, new_buffer);
        }
    }

    let mut writer = Writer::new(Path::new(&jar_out))?;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_owned();

        let buffer = match patched_classes.remove(&name) {
            Some(patched) => patched,
            None => {
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                buffer
            }
        };

        writer.write(Some(&name), &buffer)?;
    }
    report_progress(ThemeWritingEvent::Done);

    Ok(())
}

impl MyApp {
    pub fn new(
        ctx: Context,
        jar_in: Option<String>,
        jar_out: Option<String>,
    ) -> anyhow::Result<Self> {
        egui_extras::install_image_loaders(&ctx);

        use eframe::egui;
        // Start with the default fonts (we will be adding to them rather than replacing them).
        let mut fonts = egui::FontDefinitions::default();
        // Install my own font (maybe supporting non-latin characters).
        // .ttf and .otf files supported.
        fonts.font_data.insert(
            "InterRegular".to_owned(),
            egui::FontData::from_static(include_bytes!("../../assets/InterDisplay-Regular.ttf")),
        );
        fonts.font_data.insert(
            "IosevkaRegular".to_owned(),
            egui::FontData::from_static(include_bytes!(
                "../../assets/iosevka-term-curly-regular.ttf"
            )),
        );
        // Put my font first (highest priority) for proportional text:
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "InterRegular".to_owned());
        // Put my font as last fallback for monospace:
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "IosevkaRegular".to_owned());
        // Tell egui to use these fonts:
        ctx.set_fonts(fonts);

        let jar_in = jar_in.unwrap();
        let log = VecDeque::with_capacity(256);

        let (tx, rx) = mpsc::channel();

        let notifier = UiNotifier::new(ctx, tx);

        {
            let jar_in = jar_in.clone();

            let notifier = notifier.clone();
            std::thread::spawn(move || {
                let theme = load_theme_from_jar(jar_in, |prog| {
                    notifier.notify(CommonEvent::Log(LogRecord::ThemeLoading(prog)));
                })
                .unwrap();

                let mut changed_colors = HashSet::new();
                for (name, _color) in &theme.named_colors {
                    changed_colors.insert(name.clone());
                }
                notifier.notify(CommonEvent::JarParsed { theme });
            });
        }

        let mockup = Vec::from(include_bytes!("../../assets/mockup.svg"));

        let img_src: egui::ImageSource = egui::ImageSource::Bytes {
            uri: Cow::Borrowed("bytes://assets/mockup.svg"),
            bytes: egui::load::Bytes::from(mockup.clone()),
        };

        let file_dialog = FileDialog::new().add_file_filter(
            "JSON",
            Arc::new(|path| path.to_string_lossy().to_lowercase().ends_with("json")),
        );

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
            mockup: Vec::from(include_bytes!("../../assets/mockup.svg")),
            img_src,
            changed_colors: BTreeMap::new(),
            preview: Preview::new(notifier.clone(), mockup),
            rx,
            notifier,
        })
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        use eframe::egui;
        if self.first_run {
            // TODO: remove that
            ctx.set_pixels_per_point(1.5);
            self.first_run = false;
        }

        while let Ok(event) = self.rx.try_recv() {
            match event {
                CommonEvent::JarParsed { theme } => {
                    self.theme = Some(theme.clone());
                    self.preview.request_theme(theme);
                }
                CommonEvent::Log(log_record) => {
                    if self.log.len() == self.log.capacity() {
                        self.log.pop_front();
                    }
                    self.log.push_back(log_record);
                }
                CommonEvent::UpdatedImage(img_data) => {
                    ctx.forget_image("bytes://mockup.svg");
                    self.img_src = egui::ImageSource::Bytes {
                        uri: Cow::Borrowed("bytes://mockup.svg"),
                        bytes: egui::load::Bytes::from(img_data),
                    };
                }
            }
            ctx.request_repaint();
        }

        let frame = Frame::central_panel(&ctx.style());

        if self.selected_color.is_none() {
            if let Some(theme) = &self.theme {
                if theme.named_colors.contains_key("On") {
                    self.selected_color = Some("On".into());
                }
            }
        }

        egui::SidePanel::left("Palette")
            .frame(frame.inner_margin(1.0))
            .min_width(330.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(5.0);
                    ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter..."));
                    // ui.text_edit_singleline(&mut self.filter);
                    if ui.button(" X ").clicked() {
                        self.filter.clear();
                    }
                });
                ui.add_space(1.0);
                ui.separator();
                if let Some(theme) = &mut self.theme {
                    ScrollArea::vertical().show(ui, |ui| {
                        for (name, color) in &mut theme.named_colors {
                            if self.filter.trim().len() > 0 {
                                if !name.to_lowercase().contains(&self.filter.to_lowercase()) {
                                    continue;
                                }
                            }
                            if let NamedColor::Absolute(absolute_color) = color {
                                // let fill = Color32::from_rgba_unmultiplied(
                                //     absolute_color.r,
                                //     absolute_color.g,
                                //     absolute_color.b,
                                //     absolute_color.a,
                                // );

                                let mut hsva = Hsva::new(
                                    absolute_color.h,
                                    absolute_color.s,
                                    absolute_color.v,
                                    absolute_color.a,
                                );
                                ui.set_min_width(330.0);
                                if egui::Frame::none()
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.add_space(6.0);
                                            if ui.color_edit_button_hsva(&mut hsva).changed() {
                                                absolute_color.h = hsva.h;
                                                absolute_color.s = hsva.s;
                                                absolute_color.v = hsva.v;
                                                absolute_color.a = hsva.a;
                                            }
                                            ui.label(name);
                                        });
                                    })
                                    .response
                                    .interact(Sense::click())
                                    .clicked()
                                {
                                    self.selected_color = Some(name.clone());
                                }
                                ui.separator();
                            }
                        }
                    });
                }
            });

        egui::TopBottomPanel::top("Toolbar")
            .frame(frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Patch JAR").clicked() {
                        let jar_in = self.jar_in.clone();
                        let jar_out = self.jar_out.clone().unwrap();
                        let theme = self.theme.as_ref().cloned().unwrap();
                        let notifier = self.notifier.clone();
                        std::thread::spawn(move || {
                            let result = write_theme_to_jar(jar_in, jar_out, theme, |evt| {
                                notifier.notify(CommonEvent::Log(LogRecord::ThemeWriting(evt)));
                            });
                            if let Err(err) = result {
                                notifier.notify(CommonEvent::Log(LogRecord::Text(format!(
                                    "Error: {}",
                                    err
                                ))));
                            }
                        });
                    }

                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Import Berikai JSON").clicked() {
                            self.file_dialog.config_mut().default_file_filter = Some("JSON".into());
                            self.file_dialog.select_file();
                        }

                        // Update the dialog and check if the user selected a file
                        self.file_dialog.update(ctx); // FIXME: Migrate to git eframe and egui
                        if let Some(path) = self.file_dialog.take_selected() {
                            let file = File::open(path).unwrap();
                            let reader = BufReader::new(file);
                            let berikai_theme: BerikaiTheme =
                                serde_json::from_reader(reader).unwrap();
                            if let Some(theme) = self.theme.as_mut() {
                                for (name, color) in berikai_theme
                                    .window
                                    .iter()
                                    .chain(berikai_theme.arranger.iter())
                                {
                                    let rgba = HexColor::from_str(&color)
                                        .unwrap()
                                        .color()
                                        .to_srgba_unmultiplied();
                                    let hsva = Hsva::from_srgba_unmultiplied(rgba);

                                    let updated_color = NamedColor::Absolute(AbsoluteColor {
                                        h: hsva.h,
                                        s: hsva.s,
                                        v: hsva.v,
                                        a: hsva.a,
                                        compositing_mode: None,
                                    });
                                    theme
                                        .named_colors
                                        .insert(name.clone(), updated_color.clone());
                                    self.changed_colors.insert(name.clone(), updated_color);
                                }
                            }
                        }
                    });
                });
            });

        // egui::SidePanel::left("Color Picker")
        //     .frame(frame.inner_margin(8.0))
        //     .min_width(330.0)
        //     .resizable(false)
        //     .show(ctx, |_ui| {});

        egui::SidePanel::right("Debug")
            .frame(frame)
            .min_width(330.0)
            .resizable(false)
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    for rec in self.log.iter() {
                        ui.label(format!("{:?}", rec));
                    }
                    // if let Some(theme) = self.theme.read().unwrap().as_ref() {
                    //     ui.label(format!("{:#?}", theme));
                    // }
                });
            });

        egui::TopBottomPanel::bottom("Tray")
            .frame(Frame::none().fill(ctx.style().visuals.window_fill))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let (Some(color_name), Some(theme)) =
                        (&self.selected_color, self.theme.as_mut())
                    {
                        Frame::none()
                            .inner_margin(Margin::symmetric(24.0, 18.0))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.label(color_name);
                                    if let Some(NamedColor::Absolute(absolute_color)) =
                                        theme.named_colors.get_mut(color_name)
                                    {
                                        if let Some(compositing_mode) =
                                            &absolute_color.compositing_mode
                                        {
                                            ui.colored_label(
                                                ui.ctx().style().visuals.weak_text_color(),
                                                format!("Compositing: {:?}", compositing_mode),
                                            );
                                        } else {
                                            ui.colored_label(
                                                ui.ctx().style().visuals.weak_text_color(),
                                                "Compositing: Unspecified",
                                            );
                                        }

                                        if let Some(CompositingMode::RelativeToBackground) =
                                            absolute_color.compositing_mode
                                        {
                                            ui.horizontal(|ui| {
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut absolute_color.h)
                                                            .range(-360.0..=360.0)
                                                            .speed(0.1)
                                                            .prefix("ΔH"),
                                                    )
                                                    .changed()
                                                {
                                                    self.changed_colors.insert(
                                                        color_name.clone(),
                                                        NamedColor::Absolute(
                                                            absolute_color.clone(),
                                                        ),
                                                    );
                                                }
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut absolute_color.s)
                                                            .range(-1.0..=1.0)
                                                            .speed(0.01)
                                                            .prefix("ΔS"),
                                                    )
                                                    .changed()
                                                {
                                                    self.changed_colors.insert(
                                                        color_name.clone(),
                                                        NamedColor::Absolute(
                                                            absolute_color.clone(),
                                                        ),
                                                    );
                                                }
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut absolute_color.v)
                                                            .range(-1.0..=1.0)
                                                            .speed(0.01)
                                                            .prefix("ΔV"),
                                                    )
                                                    .changed()
                                                {
                                                    self.changed_colors.insert(
                                                        color_name.clone(),
                                                        NamedColor::Absolute(
                                                            absolute_color.clone(),
                                                        ),
                                                    );
                                                }
                                                ui.add_space(5.0);
                                            });
                                        } else {
                                            let mut hsva = Hsva::new(
                                                absolute_color.h,
                                                absolute_color.s,
                                                absolute_color.v,
                                                absolute_color.a,
                                            );

                                            ui.spacing_mut().slider_width = 266.0;
                                            ui.add_space(18.0);
                                            Frame::none().show(ui, |ui| {
                                                if egui::color_picker::color_picker_hsva_2d(
                                                    ui,
                                                    &mut hsva,
                                                    egui::color_picker::Alpha::OnlyBlend,
                                                ) {
                                                    absolute_color.h = hsva.h;
                                                    absolute_color.s = hsva.s;
                                                    absolute_color.v = hsva.v;
                                                    absolute_color.a = hsva.a;
                                                    self.changed_colors.insert(
                                                        color_name.clone(),
                                                        NamedColor::Absolute(
                                                            absolute_color.clone(),
                                                        ),
                                                    );
                                                }
                                            });
                                        }
                                    }
                                });
                            });
                        ui.separator();
                    }
                });
            });

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            let avail_size = ui.available_size();
            let size_diff = (avail_size - self.last_mockup_size).abs();

            if size_diff.x > 200.0 || size_diff.y > 200.0 {
                self.last_mockup_size = avail_size;
                if let Some(uri) = self.img_src.uri() {
                    ctx.forget_image(uri);
                }
            }

            if !self.changed_colors.is_empty() {
                let mut changed_colors = BTreeMap::new();
                std::mem::swap(&mut self.changed_colors, &mut changed_colors);
                self.preview.request_recolor(changed_colors);
            }

            ScrollArea::both().show(ui, |ui| {
                let s = std::time::Instant::now();
                ui.add_sized(
                    // ui.available_size() * Vec2::new(2.0, 2.0), // zoom example
                    ui.available_size(),
                    egui::Image::new(self.img_src.clone()),
                );
                let e = s.elapsed();
            });
        });
    }
}
