use std::{collections::HashMap, fmt::Debug, io::Read};

use krakatau2::{
    lib::{
        assemble,
        classfile::{
            self,
            attrs::{AttrBody, Attribute},
            code::{Bytecode, Instr, Pos},
            cpool::{BStr, Const, ConstPool},
            parse::Class,
        },
        disassemble::refprinter::{ConstData, FmimTag, PrimTag, RefPrinter, SingleTag},
        parse_utf8, AssemblerOptions, DisassemblerOptions,
    },
    zip::ZipArchive,
};
use thiserror::Error;
use tracing::{debug, warn};

use crate::{
    jar::{
        analysis::{
            find_const_name, find_method_by_sig, find_method_description, find_utf_ldc,
            init_refprinter,
        },
        goodies::{
            ColorComponents, MethodDescription, MethodSignatureKind, NamedColor,
            PaletteColorMethods, RawColorConst, RawColorConstants, RawColorGoodies,
            RawColorMethods,
        },
    },
    types::CompositingMode,
};

pub mod analysis;
pub mod debug;
pub mod goodies;
pub mod legacy;
pub mod modification;
pub mod writing;

#[derive(Debug, Error)]
pub enum ReasmError {
    #[error("Assemble error: {0:?}")]
    Assemble(krakatau2::lib::AssembleError),
    #[error("Disassemble error: {0}")]
    Disassemble(std::io::Error),
    #[error("Source parse error: {0}")]
    SourceParse(#[from] std::str::Utf8Error),
}

pub fn reasm(class: &Class<'_>) -> Result<Vec<u8>, ReasmError> {
    let mut out = Vec::new();

    krakatau2::lib::disassemble::disassemble(
        &mut out,
        &class,
        DisassemblerOptions { roundtrip: true },
    )
    .map_err(ReasmError::Disassemble)?;

    let source = std::str::from_utf8(&out)?;
    let mut assembled = assemble(source, AssemblerOptions {}).map_err(ReasmError::Assemble)?;
    let (_name, data) = assembled.pop().unwrap();

    Ok(data)
}

trait IxToInt {
    fn to_int(&self) -> u8;
}

trait IxToFloat {
    fn to_float(&self, refprinter: &RefPrinter) -> f32;
}

trait IxToDouble {
    fn to_double(&self, refprinter: &RefPrinter) -> f64;
}

impl IxToInt for Instr {
    fn to_int(&self) -> u8 {
        match self {
            Instr::Iconst0 => 0,
            Instr::Iconst1 => 1,
            Instr::Iconst2 => 2,
            Instr::Iconst3 => 3,
            Instr::Iconst4 => 4,
            Instr::Iconst5 => 5,
            Instr::Lconst0 => 0,
            Instr::Lconst1 => 1,
            Instr::Bipush(x) => *x as u8,
            Instr::Sipush(x) => *x as u8,
            x => unimplemented!("instr: {:?}", x),
        }
    }
}

impl IxToFloat for Instr {
    fn to_float(&self, refprinter: &RefPrinter) -> f32 {
        let id = match self {
            Instr::Fconst0 => return 0.0,
            Instr::Fconst1 => return 1.0,
            Instr::Fconst2 => return 2.0,
            Instr::Dconst0 => return 0.0,
            Instr::Dconst1 => return 1.0,
            Instr::Ldc(ind) => *ind as u16,
            Instr::LdcW(ind) => *ind,
            x => unimplemented!("instr: {:?}", x),
        };
        let data = refprinter.cpool.get(id as usize).unwrap();
        match &data.data {
            ConstData::Prim(_prim_tag, text) => match text.trim_end_matches("f").parse::<f32>() {
                Ok(val) => val,
                Err(err) => {
                    panic!("err parse f32 [{}]: {}", text, err);
                }
            },
            _ => unimplemented!(),
        }
    }
}

impl IxToDouble for Instr {
    fn to_double(&self, refprinter: &RefPrinter) -> f64 {
        match self {
            Instr::Fconst0 => 0.0,
            Instr::Fconst1 => 1.0,
            Instr::Fconst2 => 2.0,
            Instr::Dconst0 => 0.0,
            Instr::Dconst1 => 1.0,
            Instr::Ldc2W(ind) => {
                let data = refprinter.cpool.get(*ind as usize).unwrap();
                match &data.data {
                    ConstData::Prim(_prim_tag, text) => {
                        match text.trim_end_matches("d").parse::<f64>() {
                            Ok(val) => val,
                            Err(err) => {
                                panic!("err parse f64 [{}]: {}", text, err);
                            }
                        }
                    }
                    _ => unimplemented!(),
                }
            }
            x => unimplemented!("instr: {:?}", x),
        }
    }
}

impl MethodSignatureKind {
    fn color_name_ix_offset(&self) -> usize {
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

    fn extract_color_components(
        &self,
        idx: usize,
        bytecode: &Bytecode,
        refprinter: &RefPrinter,
    ) -> ColorComponents {
        let int = |offset: usize| bytecode.0.get(idx - offset).unwrap().1.to_int();
        let float = |offset: usize| bytecode.0.get(idx - offset).unwrap().1.to_float(refprinter);
        let double = |offset: usize| {
            bytecode
                .0
                .get(idx - offset)
                .unwrap()
                .1
                .to_double(refprinter)
        };
        match self {
            MethodSignatureKind::Si => ColorComponents::Grayscale(int(1)),
            MethodSignatureKind::Siii => ColorComponents::Rgbi(int(3), int(2), int(1)),
            MethodSignatureKind::Siiii => ColorComponents::Rgbai(int(4), int(3), int(2), int(1)),
            MethodSignatureKind::Sfff => ColorComponents::DeltaHsvf(float(3), float(2), float(1)),
            MethodSignatureKind::SRfff => unimplemented!(),
            MethodSignatureKind::SSfff => {
                let ix = &bytecode.0.get(idx - 4).unwrap().1;
                if let Instr::Ldc(ind) = ix {
                    let text = find_utf_ldc(refprinter, *ind as u16);
                    let h = float(3);
                    let s = float(2);
                    let v = float(1);
                    if let Some(color_name) = text {
                        ColorComponents::StringAndAdjust(color_name, h, s, v)
                    } else {
                        unimplemented!("string ref without text?: {:?}", ix);
                    }
                } else {
                    unimplemented!("string ref with unexpected ix: {:?}", ix);
                }
            }
            MethodSignatureKind::Ffff => {
                ColorComponents::Rgbaf(float(4), float(3), float(2), float(1))
            }
            MethodSignatureKind::Dddd => {
                ColorComponents::Rgbad(double(4), double(3), double(2), double(1))
            }
        }
    }
}

fn scan_for_named_color_defs(
    class: &Class,
    palette_color_meths: &PaletteColorMethods,
    filename: &str,
    known_colors: &mut HashMap<String, ColorComponents>,
) -> Vec<NamedColor> {
    let mut found = Vec::new();
    let rp = init_refprinter(&class.cp, &class.attrs);

    let class_name = class.cp.clsutf(class.this).and_then(parse_utf8).unwrap();

    let all_meths = palette_color_meths.all();

    for (method_idx, method) in class.methods.iter().enumerate() {
        let Some(attr) = method.attrs.first() else {
            continue;
        };
        let AttrBody::Code((code_1, _)) = &attr.body else {
            continue;
        };

        let bytecode = &code_1.bytecode;

        for (idx, (_, ix)) in bytecode.0.iter().enumerate() {
            let Instr::Invokevirtual(method_id) = ix else {
                continue;
            };
            let Some(method_descr) = find_method_description(&rp, *method_id, None) else {
                continue;
            };
            if filename.contains("dcd") {
                debug!("### METHOD_DESCR: {:?}", method_descr);
            }

            for (known_meth, compositing_mode) in &all_meths {
                if method_descr == **known_meth {
                    if let Some(sig_kind) = &known_meth.signature_kind {
                        let offset = sig_kind.color_name_ix_offset();
                        let Some((_, ix)) = bytecode.0.get(idx - offset) else {
                            debug!("{}: offset out of bounds", filename);
                            continue;
                        };
                        let ldc_id = match ix {
                            Instr::Ldc(id) => Some(*id as u16),
                            Instr::LdcW(id) => Some(*id),
                            _other => None,
                        };
                        if let Some(id) = ldc_id {
                            if filename.contains("dcd") {
                                debug!("### LDC ID IS: {:?}", id);
                            }
                            let text = find_utf_ldc(&rp, id);
                            let components = sig_kind.extract_color_components(idx, bytecode, &rp);

                            // If not in-place color name defined, then it's a method call inside other delegate method
                            // so it's not interesting to us (I guess?).
                            if let Some(color_name) = &text {
                                debug!("### FOUND COLOR: {}", color_name);
                                found.push(NamedColor {
                                    class_name: class_name.clone(),
                                    method_idx,
                                    color_name: color_name.clone(),
                                    components: components.clone(),
                                    compositing_mode: compositing_mode.clone(),
                                });
                                known_colors.insert(color_name.clone(), components);
                            } else {
                                debug!("### NOT FOUND COLOR ##################################################");
                            }
                        }
                    } else {
                        debug!("No signature kind prepared :(");
                    }
                }
            }
        }
    }

    found
}

fn extract_raw_color_goodies(class: &Class) -> Option<RawColorGoodies> {
    let rp = init_refprinter(&class.cp, &class.attrs);

    let class_name = class.cp.clsutf(class.this).and_then(parse_utf8)?;

    let mut rgbaf_desc = None;
    let mut rgbad_desc = None;

    // At first, find all popular constructors
    for method in &class.methods {
        // debug!("METH IDX: {}", method.name);
        let Some(meth_name) = class.cp.utf8(method.name).and_then(parse_utf8) else {
            continue;
        };
        // debug!("METH: {}", meth_name);
        // debug!("METH NAME: {}", meth_name);
        let Some(attr) = method.attrs.first() else {
            continue;
        };
        let AttrBody::Code((_code_1, _)) = &attr.body else {
            continue;
        };
        if meth_name != "<init>" {
            continue;
        }

        let method_id = method.desc;

        let const_line = rp.cpool.get(method_id as usize).unwrap();
        let ConstData::Utf8(utf_data) = &const_line.data else {
            panic!("Can't find method desc");
        };
        let sig = utf_data.s.to_string();

        match sig.as_str() {
            "(FFFF)V" => {
                rgbaf_desc = Some(MethodDescription {
                    class: class_name.clone(),
                    method: "<init>".into(),
                    signature: "(FFFF)V".into(),
                    signature_kind: Some(MethodSignatureKind::Ffff),
                });
            }
            "(DDDD)V" => {
                rgbad_desc = Some(MethodDescription {
                    class: class_name.clone(),
                    method: "<init>".into(),
                    signature: "(DDDD)V".into(),
                    signature_kind: Some(MethodSignatureKind::Dddd),
                });
            }
            _ => {}
        }
    }

    let raw_color_methods = RawColorMethods {
        rgba_f: rgbaf_desc.unwrap(),
        rgba_d: rgbad_desc.unwrap(),
    };

    let mut consts = Vec::new();

    // Now, find important constants in class initializer
    for method in &class.methods {
        // debug!("METH IDX: {}", method.name);
        let Some(meth_name) = class.cp.utf8(method.name).and_then(parse_utf8) else {
            continue;
        };
        // debug!("METH: {}", meth_name);
        // debug!("METH NAME: {}", meth_name);
        let Some(attr) = method.attrs.first() else {
            continue;
        };
        let AttrBody::Code((code_1, _)) = &attr.body else {
            continue;
        };
        if meth_name != "<clinit>" {
            continue;
        }

        let bytecode = &code_1.bytecode;
        for (idx, (_pos, ix)) in (bytecode.0).iter().enumerate() {
            if let Instr::Invokespecial(method_id) = ix {
                let Some(desc) = find_method_description(&rp, *method_id, None) else {
                    continue;
                };
                for raw_color_meth in raw_color_methods.all() {
                    if &desc == raw_color_meth {
                        let comps = raw_color_meth
                            .signature_kind
                            .as_ref()
                            .unwrap()
                            .extract_color_components(idx, bytecode, &rp);
                        let Instr::Putstatic(const_idx) = bytecode.0.get(idx + 1).unwrap().1 else {
                            panic!("Expected const name (Putstatic)");
                        };
                        let const_name = find_const_name(&rp, const_idx).unwrap();
                        consts.push(RawColorConst {
                            class_name: class_name.clone(),
                            const_name: const_name.clone(),
                            color_comps: comps,
                        });
                        break;
                    }
                }
                // debug!("{:?}", desc);
                // let const_line = rp.cpool.get(*method_id as usize).unwrap();
                // let ConstData::Utf8(utf_data) = &const_line.data else {
                //     panic!("Can't find method desc");
                // };
                // let sig = utf_data.s.to_string();

                // debug!("{} {:?} {:?}", pos, ix, const_line);
            }
        }
        // Static init, should contain statics initialization
    }

    Some(RawColorGoodies {
        methods: raw_color_methods,
        constants: RawColorConstants { consts },
    })
}

fn extract_palette_color_methods(class: &Class) -> Option<PaletteColorMethods> {
    // debug!("Searching palette color methods");

    let rp = init_refprinter(&class.cp, &class.attrs);

    let _class_name = class.cp.clsutf(class.this).and_then(parse_utf8)?;
    // debug!("Class >>>>> {}", class_name);

    let main_palette_method = class.methods.iter().skip(1).next()?;
    let attr = main_palette_method.attrs.first()?;
    let AttrBody::Code((code_1, _)) = &attr.body else {
        return None;
    };

    let bytecode = &code_1.bytecode;

    let invokes = bytecode.0.iter().filter_map(|(_, ix)| match ix {
        Instr::Invokevirtual(method_id) => Some(method_id),
        _ => None,
    });

    let find_method = |signature_start: &str, color_rec_name: Option<&str>, skip: Option<usize>| {
        let invokes = invokes.clone();
        invokes
            .filter_map(|method_id| {
                let method_descr = find_method_description(&rp, *method_id, color_rec_name)?;
                if method_descr.signature.starts_with(signature_start) {
                    Some(method_descr)
                } else {
                    None
                }
            })
            .skip(skip.unwrap_or_default())
            .next()
    };

    let grayscale_i = find_method("(Ljava/lang/String;I)", None, None)?;
    let color_record_class_name = grayscale_i
        .signature
        .split_once("I)L")
        .map(|(_, suffix)| suffix.strip_suffix(";"))
        .flatten()?;
    let rgb_i = find_method(
        "(Ljava/lang/String;III)",
        Some(color_record_class_name),
        None,
    )?;
    let rgba_i_absolute = find_method(
        "(Ljava/lang/String;IIII)",
        Some(color_record_class_name),
        None,
    )?;
    // TODO: search this method not by position, but by difference against rgba_i_absolute
    let rgba_i_blended_on_background = find_method(
        "(Ljava/lang/String;IIII)",
        Some(color_record_class_name),
        Some(1),
    )?;
    let hsv_f_relative_to_background = find_method(
        "(Ljava/lang/String;FFF)",
        Some(color_record_class_name),
        None,
    )?;
    let ref_hsv_f = find_method(
        &format!("(Ljava/lang/String;L{};FFF)", color_record_class_name),
        Some(color_record_class_name),
        None,
    )?;
    let name_hsv_f = find_method(
        "(Ljava/lang/String;Ljava/lang/String;FFF)",
        Some(color_record_class_name),
        None,
    )?;

    Some(PaletteColorMethods {
        grayscale_i,
        rgb_i,
        rgba_i_absolute,
        rgba_i_blended_on_background,
        hsv_f_relative_to_background,
        ref_hsv_f,
        name_hsv_f,
    })
}

fn extract_release_metadata(class: &Class) -> Option<Vec<(String, String)>> {
    // Find any strings in constant pool which contain the ": " substring
    let mut metadata = Vec::new();
    for entry in &class.cp.0 {
        if let classfile::cpool::Const::Utf8(txt) = entry {
            let parsed_string = String::from_utf8_lossy(txt.0);
            let Some((key, value)) = parsed_string.split_once(": ") else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            let key_count = key.chars().filter(|c| c.is_alphanumeric()).count();
            let value_count = value.chars().filter(|c| c.is_alphanumeric()).count();
            if value_count == 0 || key_count == 0 || key == "Not obfuscated" {
                continue;
            }
            metadata.push((key.to_string(), value.to_string()));
        }
    }

    Some(metadata)
}
