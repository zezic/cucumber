use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::jar::goodies::GeneralGoodies;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum NamedColor {
    Absolute(AbsoluteColor),
    Relative(Relative),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AbsoluteColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub compositing_mode: CompositingMode,
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
    delta_hue: f32,        // -360..360
    delta_saturation: f32, // -100..100
    delta_value: f32,      // -100..100
    delta_alpha: f32,      // -1..1
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
pub struct ThemeProcessingEvent {
    pub stage: Stage,
    pub progress: StageProgress,
}

#[derive(Debug)]
pub enum ThemeOperation {
    LoadingFromJar,
    ExtractingTheme,
    WritingToJar,
}

impl ThemeOperation {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemeOperation::LoadingFromJar => "Loading from JAR",
            ThemeOperation::ExtractingTheme => "Extracting Theme",
            ThemeOperation::WritingToJar => "Writing to JAR",
        }
    }
}

#[derive(Debug)]
pub enum Stage {
    LoadingFileNames,
    ScanningClasses,
    SearchingColorDefinitions,
    ExtractingGeneralGoodies,
    ExtractingTheme,
    WritingTheme,
}

impl Stage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Stage::LoadingFileNames => "Loading File Names",
            Stage::ScanningClasses => "Scanning Classes",
            Stage::SearchingColorDefinitions => "Searching Color Definitions",
            Stage::ExtractingGeneralGoodies => "Extracting General Goodies",
            Stage::ExtractingTheme => "Extracting Theme",
            Stage::WritingTheme => "Writing Theme",
        }
    }
}

impl From<Stage> for ThemeProcessingEvent {
    fn from(value: Stage) -> Self {
        ThemeProcessingEvent {
            stage: value,
            progress: StageProgress::Unknown,
        }
    }
}

#[derive(Debug)]
pub enum StageProgress {
    Unknown,
    Percentage(f32),
    Done,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct CucumberBitwigTheme {
    pub name: String,
    pub named_colors: BTreeMap<String, NamedColor>,
    pub constant_refs: BTreeMap<UiTarget, ColorConst>,
}

impl CucumberBitwigTheme {
    pub fn from_general_goodies(general_goodies: &GeneralGoodies) -> Self {
        let mut theme = CucumberBitwigTheme {
            name: "Extracted Theme".into(),
            ..Default::default()
        };

        let known_colors = general_goodies
            .named_colors
            .iter()
            .map(|color| (color.color_name.clone(), color.components.clone()))
            .collect();

        for color in &general_goodies.named_colors {
            let Some((r, g, b)) = color.components.to_rgb(&known_colors) else {
                warn!("Unsupported color: {:?}", color.color_name);
                continue;
            };
            let a = color.components.alpha().unwrap_or(255);
            let named_color = NamedColor::Absolute(AbsoluteColor {
                r,
                g,
                b,
                a,
                compositing_mode: color.compositing_mode.clone(),
            });
            theme
                .named_colors
                .insert(color.color_name.clone(), named_color);
        }

        if let Some(timeline_color_ref) = &general_goodies.timeline_color_ref {
            let timeline_const_name = timeline_color_ref.const_name.clone();
            let timeline_const = general_goodies
                .raw_colors
                .constants
                .consts
                .iter()
                .find(|cnst| cnst.const_name == timeline_const_name)
                .unwrap();
            if let Some((r, g, b)) = timeline_const.color_comps.to_rgb(&known_colors) {
                let a = timeline_const.color_comps.alpha().unwrap_or(255);
                let timeline_color_const = ColorConst::from_comps(r, g, b, a);
                theme
                    .constant_refs
                    .insert(UiTarget::Playhead, timeline_color_const);
            }
        }

        theme
    }
}
