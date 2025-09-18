use std::collections::HashMap;

use colorsys::{ColorTransform, Rgb, SaturationInSpace};
use krakatau2::lib::classfile::code::Instr;
use tracing::debug;

use crate::types::CompositingMode;

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

    pub fn to_rgb(
        &self,
        known_colors: &HashMap<String, ColorComponents>,
    ) -> Result<Option<(u8, u8, u8)>, String> {
        let components = match self {
            ColorComponents::Grayscale(v) => (*v, *v, *v),
            ColorComponents::Rgbi(r, g, b) => (*r, *g, *b),
            ColorComponents::Rgbai(r, g, b, _a) => (*r, *g, *b),
            ColorComponents::DeltaHsvf(..) => {
                debug!("It's dh ds dv, it's not absolute color");
                return Ok(None);
            }
            ColorComponents::RefAndAdjust(_, _, _, _) => {
                return Err("RefAndAdjust color conversion not implemented".to_string());
            }
            ColorComponents::StringAndAdjust(ref_name, h, s, v) => {
                let known = known_colors.get(ref_name).ok_or_else(|| {
                    format!("Unknown color reference: '{}'. This could be a placeholder name from a method parameter or result.", ref_name)
                })?;

                let base_rgb = match known.to_rgb(known_colors)? {
                    Some((r, g, b)) => (r, g, b),
                    None => {
                        return Err(format!(
                            "Referenced color '{}' cannot be converted to RGB (might be a delta color)",
                            ref_name
                        ));
                    }
                };

                let mut rgb = Rgb::from(base_rgb);
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
        Ok(Some(components))
    }
}

#[derive(Debug)]
pub struct FloatToAddToConstantPool {
    pub float: f32,
}

#[derive(Debug, Clone)]
pub struct NamedColor {
    pub class_name: String,
    pub method_idx: usize,
    pub color_name: String,
    pub components: ColorComponents,
    pub compositing_mode: CompositingMode,
}

#[derive(Debug)]
pub struct RawColorConst {
    pub class_name: String,
    pub const_name: String,
    pub color_comps: ColorComponents,
}

#[derive(Debug)]
pub struct RawColorConstants {
    pub consts: Vec<RawColorConst>,
}
