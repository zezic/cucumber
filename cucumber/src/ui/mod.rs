use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    fs::File,
    io::{BufReader, Read},
    path::Path,
    str::FromStr,
    sync::{Arc, RwLock},
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
use tracing::debug;
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

pub struct MyApp {
    jar_in: String,
    jar_out: Option<String>,
    log: Arc<RwLock<VecDeque<LogRecord>>>,
    theme: Arc<RwLock<Option<CucumberBitwigTheme>>>,
    selected_color: Option<String>,
    filter: String,
    first_run: bool,
    file_dialog: FileDialog,
    last_mockup_size: Vec2,
    mockup: Vec<u8>,
    img_src: egui::ImageSource<'static>,
    changed_colors: Arc<RwLock<HashSet<String>>>,
}

#[derive(Debug)]
enum LogRecord {
    ThemeLoading(ThemeLoadingEvent),
    ThemeWriting(ThemeWritingEvent),
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
        let theme = Arc::new(RwLock::new(None));
        let log = Arc::new(RwLock::new(VecDeque::with_capacity(256)));

        let changed_colors = Arc::new(RwLock::new(HashSet::new()));

        {
            let jar_in = jar_in.clone();
            let log = log.clone();
            let theme = theme.clone();
            let changed_colors = changed_colors.clone();
            std::thread::spawn(move || {
                let loaded_theme = load_theme_from_jar(jar_in, |prog| {
                    let mut log = log.write().unwrap();
                    if log.len() == log.capacity() {
                        log.pop_front();
                    }
                    log.push_back(LogRecord::ThemeLoading(prog));
                    drop(log);
                    ctx.request_repaint();
                })
                .unwrap();

                let mut theme = theme.write().unwrap();
                *theme = Some(loaded_theme.clone());
                drop(theme);

                let mut changed_colors = changed_colors.write().unwrap();
                for (name, _color) in &loaded_theme.named_colors {
                    changed_colors.insert(name.clone());
                }
                drop(changed_colors);

                ctx.request_repaint();
            });
        }

        let mockup = Vec::from(include_bytes!("../../assets/mockup.svg"));

        let img_src: egui::ImageSource = egui::ImageSource::Bytes {
            uri: Cow::Borrowed("bytes://../../assets/mockup.svg"),
            bytes: egui::load::Bytes::from(mockup.clone()),
        };

        Ok(Self {
            jar_in,
            jar_out,
            log,
            theme,
            filter: String::new(),
            selected_color: None,
            first_run: true,
            file_dialog: FileDialog::new(),
            last_mockup_size: Vec2::default(),
            mockup: Vec::from(include_bytes!("../../assets/mockup.svg")),
            img_src,
            changed_colors,
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

        let frame = Frame::central_panel(&ctx.style());

        if self.selected_color.is_none() {
            if let Some(theme) = self.theme.read().unwrap().as_ref() {
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
                let mut theme = self.theme.write().unwrap();
                if let Some(theme) = theme.as_mut() {
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
                        let theme = self.theme.read().unwrap().as_ref().cloned().unwrap();
                        let log = self.log.clone();
                        let ctx = ctx.clone();
                        std::thread::spawn(move || {
                            write_theme_to_jar(jar_in, jar_out, theme, |evt| {
                                let mut log = log.write().unwrap();
                                if log.len() == log.capacity() {
                                    log.pop_front();
                                }
                                log.push_back(LogRecord::ThemeWriting(evt));
                                drop(log);
                                ctx.request_repaint();
                            })
                        });
                    }

                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Import Berikai JSON").clicked() {
                            self.file_dialog.select_file();
                        }

                        // Update the dialog and check if the user selected a file
                        self.file_dialog.update(ctx);
                        if let Some(path) = self.file_dialog.take_selected() {
                            let file = File::open(path).unwrap();
                            let reader = BufReader::new(file);
                            let berikai_theme: BerikaiTheme =
                                serde_json::from_reader(reader).unwrap();
                            if let Some(theme) = self.theme.write().unwrap().as_mut() {
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
                                    theme.named_colors.insert(
                                        name.clone(),
                                        NamedColor::Absolute(AbsoluteColor {
                                            h: hsva.h,
                                            s: hsva.s,
                                            v: hsva.v,
                                            a: hsva.a,
                                            compositing_mode: None,
                                        }),
                                    );
                                    self.changed_colors.write().unwrap().insert(name.clone());
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
                let log = self.log.read().unwrap();

                ScrollArea::vertical().show(ui, |ui| {
                    for rec in log.iter() {
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
                    if let Some(color_name) = &self.selected_color {
                        Frame::none()
                            .inner_margin(Margin::symmetric(24.0, 18.0))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.label(color_name);
                                    let mut theme = self.theme.write().unwrap();
                                    if let Some(NamedColor::Absolute(absolute_color)) =
                                        theme.as_mut().unwrap().named_colors.get_mut(color_name)
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
                                                    self.changed_colors
                                                        .write()
                                                        .unwrap()
                                                        .insert(color_name.clone());
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
                                                    self.changed_colors
                                                        .write()
                                                        .unwrap()
                                                        .insert(color_name.clone());
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
                                                    self.changed_colors
                                                        .write()
                                                        .unwrap()
                                                        .insert(color_name.clone());
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
                                                    self.changed_colors
                                                        .write()
                                                        .unwrap()
                                                        .insert(color_name.clone());
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
            if avail_size != self.last_mockup_size
                || !self.changed_colors.read().unwrap().is_empty()
            {
                self.last_mockup_size = avail_size;

                let uri = self.img_src.uri().unwrap();
                ctx.forget_image(uri);

                if let Some(theme) = self.theme.read().unwrap().as_ref() {
                    // Step 1: Parse the SVG XML
                    let mut root = Element::parse(self.mockup.as_slice()).unwrap();
                    // Step 2: Traverse and modify elements with the target class
                    fn modify_element(element: &mut Element, target_class: &str, new_fill: &str) {
                        if let Some(class) = element.attributes.get("class") {
                            if class == target_class {
                                if element.attributes.contains_key("fill") {
                                    element
                                        .attributes
                                        .insert("fill".to_string(), new_fill.to_string());
                                } else {
                                    element
                                        .attributes
                                        .insert("stop-color".to_string(), new_fill.to_string());
                                }
                            }
                        }

                        // Recursively process child elements
                        for child in element.children.iter_mut() {
                            if let xmltree::XMLNode::Element(ref mut child_element) = child {
                                modify_element(child_element, target_class, new_fill);
                            }
                        }
                    }
                    fn modify_element_relative(
                        element: &mut Element,
                        target_class: &str,
                        dh: f32,
                        ds: f32,
                        dv: f32,
                        theme: &CucumberBitwigTheme,
                    ) {
                        if let Some(class) = element.attributes.get("class") {
                            if class == target_class {
                                if let Some(bg) = element.attributes.get("bg") {
                                    if let Some((_, NamedColor::Absolute(absolute_color))) =
                                        theme.named_colors.iter().find(|(name, _b)| {
                                            &name.to_lowercase().replace(" ", "-") == bg
                                        })
                                    {
                                        let [r, g, b] = Hsva::new(
                                            absolute_color.h,
                                            absolute_color.s,
                                            absolute_color.v,
                                            1.0,
                                        )
                                        .to_srgb();
                                        let rgb =
                                            colorsys::Rgb::from((r as f64, g as f64, b as f64));
                                        let hsl = colorsys::Hsl::from(rgb);
                                        let h = (hsl.hue() + dh as f64).rem_euclid(360.0);
                                        let s = (hsl.saturation() / 100.0 + ds as f64)
                                            .clamp(0.0, 1.0)
                                            * 100.0;
                                        let l = (hsl.lightness() / 100.0 + dv as f64 * 0.5)
                                            .clamp(0.0, 1.0)
                                            * 100.0;
                                        let hsl = colorsys::Hsl::new(h, s, l, None);
                                        let rgb = colorsys::Rgb::from(hsl);

                                        let new_fill = format!(
                                            "#{:02X}{:02X}{:02X}{:02X}",
                                            (rgb.red()) as u8,
                                            (rgb.green()) as u8,
                                            (rgb.blue()) as u8,
                                            255
                                        );
                                        element
                                            .attributes
                                            .insert("fill".to_string(), new_fill.to_string());
                                    }
                                }
                            }
                        }

                        // Recursively process child elements
                        for child in element.children.iter_mut() {
                            if let xmltree::XMLNode::Element(ref mut child_element) = child {
                                modify_element_relative(
                                    child_element,
                                    target_class,
                                    dh,
                                    ds,
                                    dv,
                                    &theme,
                                );
                            }
                        }
                    }
                    for changed_color in self.changed_colors.write().unwrap().drain() {
                        if let NamedColor::Absolute(repl) =
                            theme.named_colors.get(&changed_color).as_ref().unwrap()
                        {
                            if let Some(CompositingMode::RelativeToBackground) =
                                repl.compositing_mode
                            {
                                modify_element_relative(
                                    &mut root,
                                    &changed_color.to_lowercase().replace(" ", "-"),
                                    repl.h,
                                    repl.s,
                                    repl.v,
                                    &theme,
                                );
                            } else {
                                let [r, g, b, a] = Hsva::new(repl.h, repl.s, repl.v, repl.a)
                                    .to_srgba_unmultiplied();
                                let hex = format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a);
                                modify_element(
                                    &mut root,
                                    &changed_color.to_lowercase().replace(" ", "-"),
                                    &hex,
                                );
                            }
                        }
                    }
                    // Step 3: Serialize the modified SVG back to bytes
                    let mut output = Vec::new();
                    let config = EmitterConfig::new().perform_indent(true); // Optional pretty printing
                    root.write_with_config(&mut output, config).unwrap();
                    self.mockup = output;
                }

                self.img_src = egui::ImageSource::Bytes {
                    uri: Cow::Borrowed("bytes://../../assets/mockup.svg"),
                    bytes: egui::load::Bytes::from(self.mockup.clone()),
                };
            }

            ScrollArea::both().show(ui, |ui| {
                ui.add_sized(
                    // ui.available_size() * Vec2::new(2.0, 2.0),
                    ui.available_size(),
                    egui::Image::new(self.img_src.clone()),
                );
            });

        });
    }
}
