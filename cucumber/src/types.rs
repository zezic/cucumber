use std::collections::BTreeMap;

use krakatau2::zip::ZipArchive;
use serde::{Deserialize, Serialize};

use crate::extract_general_goodies;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum NamedColor {
    Absolute(AbsoluteColor),
    Relative(Relative),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AbsoluteColor {
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub a: f32,
    pub compositing_mode: Option<CompositingMode>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum CompositingMode {
    Absolute,
    RelativeToBackground,
    BlendedOnBackground,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Relative {
    base: RelativeColorBase,
    delta_hue: f32, // -360..360
    delta_saturation: f32, // -100..100
    delta_value: f32, // -100..100
    delta_alpha: f32, // -1..1
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum RelativeColorBase {
    Internal(String), // Use color defined in main Bitwig palette
    External(String), // Use color defined in external resource
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UiTarget {
    Playhead,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ColorConst {
    Black,
    White,
    Gray,
    DarkGray,
    LightGray,
    Red,
    Orange,
    Green,
    Blue,
    Yellow,
    Transparent,
    Violet,
}

impl ColorConst {
    pub fn from_comps(r: u8, g: u8, b: u8, a: u8) -> Self {
        use ColorConst::*;
        match (r, g, b, a) {
            (0, 0, 0, 255) => Black,
            (255, 255, 255, 255) => White,
            (127, 127, 127, 255) => Gray,
            (63, 63, 63, 255) => DarkGray,
            (191, 191, 191, 255) => LightGray,
            (255, 0, 0, 255) => Red,
            (255, 140, 0, 255) => Orange,
            (0, 255, 0, 255) => Green,
            (0, 0, 255, 255) => Blue,
            (255, 255, 0, 255) => Yellow,
            (0, 0, 0, 0) => Transparent,
            (169, 169, 254, 255) => Violet,
            x => panic!("can't build color const from {:?}", x),
        }
    }
}

#[derive(Debug)]
pub enum ThemeLoadingEvent {
    Aaa,
    Bbb,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct CucumberBitwigTheme {
    pub name: String,
    pub named_colors: BTreeMap<String, NamedColor>,
    pub constant_refs: BTreeMap<UiTarget, ColorConst>,
}

impl CucumberBitwigTheme {
    pub fn from_jar< R: std::io::Read + std::io::Seek >(zip: &mut ZipArchive<R>, report_progress: impl FnMut(ThemeLoadingEvent)) -> Self {
        let general_goodies = extract_general_goodies(zip, report_progress).unwrap();

        let mut theme = CucumberBitwigTheme {
            name: "Extracted Theme".into(),
            ..Default::default()
        };

        let known_colors = general_goodies.named_colors.iter().map(|color| {
            (color.color_name.clone(), color.components.clone())
        }).collect();

        for color in general_goodies.named_colors {
            let (h, s, v) = color.components.to_hsv(&known_colors);
            let a = color.components.alpha().unwrap_or(255);
            let named_color = NamedColor::Absolute(
                AbsoluteColor {
                    h, s, v, a: a as f32 / 255.0,
                    compositing_mode: color.compositing_mode
                }
            );
            theme.named_colors.insert(color.color_name.clone(), named_color);
        }

        if let Some(timeline_color_ref) = general_goodies.timeline_color_ref {
            let timeline_const_name = timeline_color_ref.const_name;
            let timeline_const = general_goodies.raw_colors.constants.consts.iter().find(|cnst| {
                cnst.const_name == timeline_const_name
            }).unwrap();
            if let Some((r, g, b)) = timeline_const.color_comps.to_rgb(&known_colors) {
                let a = timeline_const.color_comps.alpha().unwrap_or(255);
                let timeline_color_const = ColorConst::from_comps(r, g, b, a);
                theme.constant_refs.insert(UiTarget::Playhead, timeline_color_const);
            }
        }

        theme
    }
}