use krakatau2::lib::{
    classfile::code::{Bytecode, Instr},
    disassemble::refprinter::RefPrinter,
};

use crate::{
    jar::{
        analysis::introspection::find_utf_ldc,
        core::bytecode::{IxToDouble, IxToFloat, IxToInt},
        types::colors::ColorComponents,
    },
    types::CompositingMode,
};

#[derive(Debug)]
pub enum ColorExtractionError {
    BytecodeOutOfBounds {
        requested_offset: usize,
        idx: usize,
        bytecode_len: usize,
    },
    UnexpectedInstruction {
        expected: String,
        found: String,
        offset: usize,
        idx: usize,
    },
    StringResolutionFailed {
        ldc_index: u16,
        instruction: String,
    },
    NotImplemented {
        feature: String,
        signature_kind: String,
        context: String,
    },
}

impl std::fmt::Display for ColorExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColorExtractionError::BytecodeOutOfBounds {
                requested_offset,
                idx,
                bytecode_len,
            } => {
                write!(
                    f,
                    "Bytecode out of bounds: tried to access offset {} from index {} (bytecode length: {})",
                    requested_offset, idx, bytecode_len
                )
            }
            ColorExtractionError::UnexpectedInstruction {
                expected,
                found,
                offset,
                idx,
            } => {
                write!(
                    f,
                    "Unexpected instruction at offset {} from index {}: expected {}, found {}",
                    offset, idx, expected, found
                )
            }
            ColorExtractionError::StringResolutionFailed {
                ldc_index,
                instruction,
            } => {
                write!(
                    f,
                    "Failed to resolve string from LDC instruction {} with index {}",
                    instruction, ldc_index
                )
            }
            ColorExtractionError::NotImplemented {
                feature,
                signature_kind,
                context,
            } => {
                write!(
                    f,
                    "Feature '{}' not implemented for signature kind {} in context: {}",
                    feature, signature_kind, context
                )
            }
        }
    }
}

impl std::error::Error for ColorExtractionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDescription {
    pub class: String,
    pub method: String,
    pub signature: String,
    pub signature_kind: Option<MethodSignatureKind>,
}

impl MethodSignatureKind {
    pub fn color_name_ix_offset(&self) -> usize {
        match self {
            MethodSignatureKind::Si => 2,
            MethodSignatureKind::Siii => 4,
            MethodSignatureKind::Siiii => 5,
            MethodSignatureKind::Sfff => 4,
            MethodSignatureKind::SRfff => 6,
            MethodSignatureKind::SSfff => 5,
            MethodSignatureKind::Ffff | MethodSignatureKind::Dddd => unreachable!(),
        }
    }

    pub fn extract_color_components(
        &self,
        idx: usize,
        bytecode: &Bytecode,
        refprinter: &RefPrinter,
    ) -> Result<ColorComponents, ColorExtractionError> {
        let signature_kind_str = format!("{:?}", self);

        // Safe helper functions with proper error handling
        let safe_get_instruction = |offset: usize| -> Result<&Instr, ColorExtractionError> {
            if idx < offset {
                return Err(ColorExtractionError::BytecodeOutOfBounds {
                    requested_offset: offset,
                    idx,
                    bytecode_len: bytecode.0.len(),
                });
            }
            bytecode.0.get(idx - offset).map(|(_, instr)| instr).ok_or(
                ColorExtractionError::BytecodeOutOfBounds {
                    requested_offset: offset,
                    idx,
                    bytecode_len: bytecode.0.len(),
                },
            )
        };

        let int = |offset: usize| -> Result<u8, ColorExtractionError> {
            safe_get_instruction(offset).map(|instr| instr.to_int())
        };

        let float = |offset: usize| -> Result<f32, ColorExtractionError> {
            safe_get_instruction(offset).map(|instr| instr.to_float(refprinter))
        };

        let double = |offset: usize| -> Result<f64, ColorExtractionError> {
            safe_get_instruction(offset).map(|instr| instr.to_double(refprinter))
        };

        match self {
            MethodSignatureKind::Si => Ok(ColorComponents::Grayscale(int(1)?)),
            MethodSignatureKind::Siii => Ok(ColorComponents::Rgbi(int(3)?, int(2)?, int(1)?)),
            MethodSignatureKind::Siiii => {
                Ok(ColorComponents::Rgbai(int(4)?, int(3)?, int(2)?, int(1)?))
            }
            MethodSignatureKind::Sfff => {
                Ok(ColorComponents::DeltaHsvf(float(3)?, float(2)?, float(1)?))
            }
            MethodSignatureKind::SRfff => Err(ColorExtractionError::NotImplemented {
                feature: "SRfff color reference extraction".to_string(),
                signature_kind: signature_kind_str,
                context: "Reference-based HSV with color object parameter".to_string(),
            }),
            MethodSignatureKind::SSfff => {
                let instr = safe_get_instruction(4)?;
                match instr {
                    Instr::Ldc(ind) => {
                        let text = find_utf_ldc(refprinter, *ind as u16);
                        let h = float(3)?;
                        let s = float(2)?;
                        let v = float(1)?;

                        if let Some(color_name) = text {
                            Ok(ColorComponents::StringAndAdjust(color_name, h, s, v))
                        } else {
                            Err(ColorExtractionError::StringResolutionFailed {
                                ldc_index: *ind as u16,
                                instruction: format!("{:?}", instr),
                            })
                        }
                    }
                    Instr::Aload0 | Instr::Aload1 | Instr::Aload2 | Instr::Aload3 => {
                        // String comes from method parameter or local variable
                        // We can't resolve the actual string value at bytecode analysis time,
                        // so we'll use a placeholder name and still extract the HSV values
                        let h = float(3)?;
                        let s = float(2)?;
                        let v = float(1)?;
                        let placeholder_name = format!("Parameter_{:?}_at_idx_{}", instr, idx);
                        Ok(ColorComponents::StringAndAdjust(placeholder_name, h, s, v))
                    }
                    Instr::Invokevirtual(_) => {
                        // String comes from a method call result
                        // Similar to parameter case - we can't resolve it statically
                        let h = float(3)?;
                        let s = float(2)?;
                        let v = float(1)?;
                        let placeholder_name = format!("MethodResult_at_idx_{}", idx);
                        Ok(ColorComponents::StringAndAdjust(placeholder_name, h, s, v))
                    }
                    other => Err(ColorExtractionError::UnexpectedInstruction {
                        expected: "Ldc, Aload, or Invokevirtual instruction for string value"
                            .to_string(),
                        found: format!("{:?}", other),
                        offset: 4,
                        idx,
                    }),
                }
            }
            MethodSignatureKind::Ffff => Ok(ColorComponents::Rgbaf(
                float(4)?,
                float(3)?,
                float(2)?,
                float(1)?,
            )),
            MethodSignatureKind::Dddd => Ok(ColorComponents::Rgbad(
                double(4)?,
                double(3)?,
                double(2)?,
                double(1)?,
            )),
        }
    }
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
