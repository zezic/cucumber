use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub enum NamedColor {
    Absolute(AbsoluteColor),
    Relative(Relative),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AbsoluteColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Relative {
    base: RelativeColorBase,
    delta_hue: f32, // -360..360
    delta_saturation: f32, // -100..100
    delta_value: f32, // -100..100
    delta_alpha: f32,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum RelativeColorBase {
    Internal(String), // Use color defined in main Bitwig palette
    External(String), // Use color defined in external resource

}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum UiTarget {
    Playhead,
}

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Default, Debug, Deserialize, Serialize)]
pub struct CucumberBitwigTheme {
    pub name: String,
    pub named_colors: HashMap<String, NamedColor>,
    pub constant_refs: HashMap<UiTarget, ColorConst>,
}