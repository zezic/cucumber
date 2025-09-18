use std::collections::HashMap;

use colorsys::{ColorTransform, Rgb, SaturationInSpace};
use krakatau2::lib::classfile::code::Instr;
use tracing::debug;

use crate::{
    jar::{analysis::NamedColorGetterInvocation, legacy::TimelineColorReference},
    types::CompositingMode,
};

#[derive(Debug)]
pub struct GeneralGoodies {
    pub init_class: String,
    pub named_colors: Vec<NamedColor>,
    pub palette_color_methods: PaletteColorMethods,
    pub raw_colors: RawColorGoodies,
    pub timeline_color_ref: Option<TimelineColorReference>, // Don't exist on 5.2.4?
    pub release_metadata: Vec<(String, String)>,
    pub named_color_getter_1: MethodDescription,
    pub named_color_getter_invocations: Vec<(String, NamedColorGetterInvocation)>,
}

#[derive(Debug, Clone)]
pub struct NamedColor {
    pub class_name: String,
    pub method_idx: usize,
    pub color_name: String,
    pub components: ColorComponents,
    pub compositing_mode: CompositingMode,
}

#[derive(Clone, Debug)]
pub enum ColorComponents {
    Grayscale(u8),
    Rgbi(u8, u8, u8),
    Rgbai(u8, u8, u8, u8),
    DeltaHsvf(f32, f32, f32),
    Rgbaf(f32, f32, f32, f32),
    Rgbad(f64, f64, f64, f64),
    #[allow(dead_code)]
    RefAndAdjust(String, f32, f32, f32),
    StringAndAdjust(String, f32, f32, f32),
}
impl ColorComponents {
    pub fn alpha(&self) -> Option<u8> {
        Some(match self {
            ColorComponents::Grayscale(_) => 255,
            ColorComponents::Rgbi(_, _, _) => 255,
            ColorComponents::Rgbai(_, _, _, a) => *a,
            ColorComponents::DeltaHsvf(_, _, _) => 255,
            ColorComponents::Rgbaf(_, _, _, a) => (a * 255.0) as u8,
            ColorComponents::Rgbad(_, _, _, a) => (a * 255.0) as u8,
            ColorComponents::RefAndAdjust(_, _, _, _) => return None,
            ColorComponents::StringAndAdjust(_, _, _, _) => return None,
        })
    }

    pub fn to_ixs(
        &self,
        next_free_cpool_idx: usize,
    ) -> (Vec<Instr>, Option<Vec<FloatToAddToConstantPool>>) {
        match self {
            ColorComponents::Rgbai(r, g, b, a) => {
                let mut ixs = vec![];
                for comp in [r, g, b, a] {
                    if *comp > 127 {
                        ixs.push(Instr::Sipush(*comp as i16));
                    } else {
                        ixs.push(Instr::Bipush(*comp as i8));
                    }
                }
                (ixs, None)
            }
            ColorComponents::DeltaHsvf(h, s, v) => {
                let mut ixs = vec![];
                let mut floats = vec![];
                for (idx, comp) in [h, s, v].into_iter().enumerate() {
                    let cpool_idx = next_free_cpool_idx + idx;
                    if cpool_idx > 255 {
                        ixs.push(Instr::LdcW(cpool_idx as u16));
                    } else {
                        ixs.push(Instr::Ldc(cpool_idx as u8));
                    }
                    floats.push(FloatToAddToConstantPool { float: *comp });
                }
                (ixs, Some(floats))
            }
            _ => todo!(),
        }
    }

    pub fn to_rgb(&self, known_colors: &HashMap<String, ColorComponents>) -> Option<(u8, u8, u8)> {
        let components = match self {
            ColorComponents::Grayscale(v) => (*v, *v, *v),
            ColorComponents::Rgbi(r, g, b) => (*r, *g, *b),
            ColorComponents::Rgbai(r, g, b, _a) => (*r, *g, *b),
            ColorComponents::DeltaHsvf(..) => {
                debug!("It's dh ds dv, it's not absolute color");
                return None;
            }
            ColorComponents::RefAndAdjust(_, _, _, _) => todo!(),
            ColorComponents::StringAndAdjust(ref_name, h, s, v) => {
                let Some(known) = known_colors.get(ref_name) else {
                    panic!("Unknown color ref: {}", ref_name);
                };
                let Some((r, g, b)) = known.to_rgb(&known_colors) else {
                    return None;
                };
                let mut rgb = Rgb::from((r, g, b));
                rgb.adjust_hue(*h as f64);
                rgb.saturate(SaturationInSpace::Hsl(*s as f64 * 100.));
                rgb.lighten(*v as f64 * 100.);
                rgb.into()
            }
            ColorComponents::Rgbaf(r, g, b, _a) => {
                ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
            }
            ColorComponents::Rgbad(r, g, b, _a) => {
                ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
            }
        };
        Some(components)
    }
}

#[derive(Debug)]
pub struct FloatToAddToConstantPool {
    pub float: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDescription {
    pub class: String,
    pub method: String,
    pub signature: String,
    pub signature_kind: Option<MethodSignatureKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodSignatureKind {
    Si,
    Siii,
    Siiii,
    Sfff,
    SRfff, // R - reference to other, already defined color
    SSfff,
    Ffff,
    Dddd,
}

// Color methods and defined static colors (contain important black color)
#[derive(Debug)]
pub struct RawColorGoodies {
    #[allow(dead_code)]
    pub methods: RawColorMethods,
    pub constants: RawColorConstants,
}

// Color methods and defined static colors (contain important black color)
#[derive(Debug)]
pub struct RawColorMethods {
    // rgb_i: MethodDescription,
    // grayscale_i: MethodDescription,
    // rgb_f: MethodDescription,
    pub rgba_f: MethodDescription,
    // rgb_d: MethodDescription,
    pub rgba_d: MethodDescription,
}

impl RawColorMethods {
    pub fn all(&self) -> Vec<&MethodDescription> {
        vec![&self.rgba_f, &self.rgba_d]
    }
}

#[derive(Debug)]
pub struct RawColorConstants {
    pub consts: Vec<RawColorConst>,
}

#[derive(Debug)]
pub struct RawColorConst {
    pub class_name: String,
    pub const_name: String,
    pub color_comps: ColorComponents,
}

#[derive(Debug)]
pub struct PaletteColorMethods {
    pub grayscale_i: MethodDescription,
    pub rgb_i: MethodDescription,
    pub rgba_i_absolute: MethodDescription,
    pub rgba_i_blended_on_background: MethodDescription,
    // H - 0..360 or -360, s 0..1, v -1..+1
    // By default used only for:
    // Light button stroke - ???
    // Selected borderless button background - used but where ???
    // Pressed borderless button background - not used
    // Rubber Button Emboss Highlight - not used
    // Icon Frame - used, but where?
    // Slider background - used, but where?
    // Knob Body - used very much
    // Knob Value Background
    // Knob Value Background (dark)
    //
    pub hsv_f_relative_to_background: MethodDescription,
    pub ref_hsv_f: MethodDescription,
    pub name_hsv_f: MethodDescription,
}

impl PaletteColorMethods {
    pub fn all(&self) -> Vec<(&MethodDescription, CompositingMode)> {
        vec![
            (&self.grayscale_i, CompositingMode::Absolute),
            (&self.rgb_i, CompositingMode::Absolute),
            (&self.rgba_i_absolute, CompositingMode::Absolute),
            (
                &self.rgba_i_blended_on_background,
                CompositingMode::BlendedOnBackground,
            ),
            (
                &self.hsv_f_relative_to_background,
                CompositingMode::RelativeToBackground,
            ),
            (&self.ref_hsv_f, CompositingMode::Absolute),
            (&self.name_hsv_f, CompositingMode::Absolute),
        ]
    }

    pub fn from_components(&self, comps: &ColorComponents) -> &MethodDescription {
        match comps {
            ColorComponents::Grayscale(_) => &self.grayscale_i,
            ColorComponents::Rgbi(_, _, _) => &self.rgb_i,
            ColorComponents::Rgbai(_, _, _, _) => &self.rgba_i_absolute,
            ColorComponents::DeltaHsvf(_, _, _) => &self.hsv_f_relative_to_background,
            ColorComponents::Rgbaf(_, _, _, _) => unreachable!(),
            ColorComponents::Rgbad(_, _, _, _) => unreachable!(),
            ColorComponents::RefAndAdjust(_, _, _, _) => &self.ref_hsv_f,
            ColorComponents::StringAndAdjust(_, _, _, _) => &self.name_hsv_f,
        }
    }
}
