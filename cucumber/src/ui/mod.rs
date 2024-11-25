use std::{
    collections::{HashMap, VecDeque}, io::Read, path::Path, sync::{Arc, RwLock}
};

use eframe::{
    egui::{Context, Frame, ScrollArea},
    App,
};
use krakatau2::{file_output_util::Writer, lib::{classfile, ParserOptions}, zip};
use anyhow::anyhow;

use crate::{extract_general_goodies, patching::patch_class, reasm, replace_named_color, types::{CucumberBitwigTheme, NamedColor, ThemeLoadingEvent}, ColorComponents};

pub struct MyApp {
    jar_in: String,
    jar_out: Option<String>,
    log: Arc<RwLock<VecDeque<LogRecord>>>,
    theme: Arc<RwLock<Option<CucumberBitwigTheme>>>,
    filter: String,
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
            if replace_named_color(
                &mut class,
                &clr.color_name,
                ColorComponents::Rgbai(
                    repl.r,
                    repl.g,
                    repl.b,
                    repl.a,
                ),
                &mut general_goodies.named_colors,
                &general_goodies.palette_color_methods,
            )
            .is_none()
            {
                println!("failed to replace in {}", file_name_w_ext);
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
        let jar_in = jar_in.unwrap();
        let theme = Arc::new(RwLock::new(None));
        let log = Arc::new(RwLock::new(VecDeque::with_capacity(256)));
        {
            let jar_in = jar_in.clone();
            let log = log.clone();
            let theme = theme.clone();
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
                *theme = Some(loaded_theme);
                ctx.request_repaint();
            });
        }
        Ok(Self {
            jar_in,
            jar_out,
            log,
            theme,
            filter: String::new(),
        })
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        use eframe::egui;
        egui_extras::install_image_loaders(ctx);

        // TODO: remove that
        ctx.set_pixels_per_point(1.5);

        let frame = Frame::central_panel(&ctx.style());

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
                                let mut rgba_unmul = [
                                    absolute_color.r,
                                    absolute_color.g,
                                    absolute_color.b,
                                    absolute_color.a,
                                ];
                                ui.horizontal(|ui| {
                                    ui.add_space(6.0);
                                    if ui
                                        .color_edit_button_srgba_unmultiplied(&mut rgba_unmul)
                                        .changed()
                                    {
                                        absolute_color.r = rgba_unmul[0];
                                        absolute_color.g = rgba_unmul[1];
                                        absolute_color.b = rgba_unmul[2];
                                        absolute_color.a = rgba_unmul[3];
                                    }
                                    ui.label(name);
                                });
                                ui.separator();
                            }
                        }
                    });
                }
            });

        egui::SidePanel::right("Debug")
            .frame(frame)
            .min_width(330.0)
            .resizable(false)
            .show(ctx, |ui| {
                let log = self.log.read().unwrap();
                for rec in log.iter() {
                    ui.label(format!("{:?}", rec));
                }
            });

        egui::TopBottomPanel::top("Toolbar").frame(frame).show(ctx, |ui| {
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
        });

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            if ui.button("hehehe").clicked() {
                println!("CLICK");
            }
        });
    }
}
