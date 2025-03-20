use std::{
    collections::{BTreeMap, VecDeque},
    sync::mpsc::{self, Receiver, Sender},
};

use eframe::epaint::Hsva;
use tracing::info;
use xml::EmitterConfig;
use xmltree::Element;

use crate::types::{CompositingMode, CucumberBitwigTheme, NamedColor};

use super::UiNotifier;

pub struct Preview {
    tx: Sender<UpdateRequest>,
}

impl Preview {
    pub fn new(notifier: UiNotifier, svg_data: Vec<u8>) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("Preview".into())
            .spawn(|| worker(svg_data, rx, notifier))
            .unwrap();
        Self { tx }
    }

    pub fn request_recolor(&self, changed_colors: BTreeMap<String, NamedColor>) {
        self.tx
            .send(UpdateRequest::Recolor { changed_colors })
            .unwrap();
    }

    pub fn request_theme(&self, theme: CucumberBitwigTheme) {
        self.tx
            .send(UpdateRequest::Theme { new_theme: theme })
            .unwrap();
    }
}

enum UpdateRequest {
    Recolor {
        changed_colors: BTreeMap<String, NamedColor>,
    },
    Theme {
        new_theme: CucumberBitwigTheme,
    },
}

impl UpdateRequest {
    fn same(&self, other: &UpdateRequest) -> bool {
        match (self, other) {
            (
                UpdateRequest::Recolor {
                    changed_colors: colors_1,
                },
                UpdateRequest::Recolor {
                    changed_colors: colors_2,
                },
            ) => colors_1.keys().zip(colors_2.keys()).all(|(a, b)| a == b),
            _ => false,
        }
    }
}

fn worker(mut svg_data: Vec<u8>, rx: Receiver<UpdateRequest>, notifier: UiNotifier) {
    let mut theme = None;
    let mut queue = VecDeque::new();
    loop {
        let req = rx.recv().expect("recv UpdateRequest");
        queue.push_back(req);
        while let Ok(extra_req) = rx.try_recv() {
            let back = queue.back().unwrap();
            if extra_req.same(back) {
                queue.pop_back();
            }
            queue.push_back(extra_req);
        }
        while let Some(req) = queue.pop_front() {
            match req {
                UpdateRequest::Recolor { changed_colors } => {
                    if let Some(theme) = theme.as_mut() {
                        recolor(&mut svg_data, theme, changed_colors);
                        notifier.notify(super::CommonEvent::UpdatedImage(svg_data.clone()));
                    }
                }
                UpdateRequest::Theme { new_theme } => theme = Some(new_theme),
            }
        }
    }
}

fn recolor(
    mut svg_data: &mut Vec<u8>,
    theme: &mut CucumberBitwigTheme,
    changed_colors: BTreeMap<String, NamedColor>,
) {
    info!("Recoloring: {:?}", changed_colors);
    // Step 1: Parse the SVG XML
    let mut root = Element::parse(svg_data.as_slice()).unwrap();

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
                    if let Some((_, NamedColor::Absolute(absolute_color))) = theme
                        .named_colors
                        .iter()
                        .find(|(name, _b)| &name.to_lowercase().replace(" ", "-") == bg)
                    {
                        let [r, g, b] =
                            Hsva::new(absolute_color.h, absolute_color.s, absolute_color.v, 1.0)
                                .to_srgb();
                        let rgb = colorsys::Rgb::from((r as f64, g as f64, b as f64));
                        let hsl = colorsys::Hsl::from(rgb);
                        let h = (hsl.hue() + dh as f64).rem_euclid(360.0);
                        let s = (hsl.saturation() / 100.0 + ds as f64).clamp(0.0, 1.0) * 100.0;
                        let l = (hsl.lightness() / 100.0 + dv as f64 * 0.5).clamp(0.0, 1.0) * 100.0;
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
                modify_element_relative(child_element, target_class, dh, ds, dv, &theme);
            }
        }
    }

    for (changed_color, named_color) in changed_colors.into_iter() {
        if let NamedColor::Absolute(repl) = named_color {
            if let Some(CompositingMode::RelativeToBackground) = repl.compositing_mode {
                modify_element_relative(
                    &mut root,
                    &changed_color.to_lowercase().replace(" ", "-"),
                    repl.h,
                    repl.s,
                    repl.v,
                    &theme,
                );
            } else {
                let [r, g, b, a] =
                    Hsva::new(repl.h, repl.s, repl.v, repl.a).to_srgba_unmultiplied();
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
    let config = EmitterConfig::new().perform_indent(true);
    // Optional pretty printing

    svg_data.clear();
    root.write_with_config(&mut svg_data, config).unwrap();
}
