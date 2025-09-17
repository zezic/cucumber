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
        goodies::{
            ColorComponents, MethodDescription, MethodSignatureKind, NamedColor,
            PaletteColorMethods, RawColorConst, RawColorConstants, RawColorGoodies,
            RawColorMethods,
        },
        legacy::TimelineColorReference,
    },
    types::CompositingMode,
};

pub mod analysis;
pub mod debug;
pub mod goodies;
pub mod legacy;
pub mod writing;

// Will search constant pool for that (inside Utf8 entry)
// Contain most of the colors and methods to set them
const PALETTE_ANCHOR: &str = "Device Tint Future";

// Contain time-bomb initialization around constant 5000
const INIT_ANCHOR: &str = "Apply Device Remote Control Changes To All Devices";

// Contain crash report builder where we can get some info about release version
const CRASH_REPORT_ANCHOR: &str = "stack trace.txt";

// Contain named color getter method – the one which is used to define arranger BG colors
// It accepts string and returns RAW_COLOR class, easy to identify
const NAMED_COLOR_GETTER_1_ANCHOR: &str = "Remove all user input simulations";

// Other color anchor
// const OTHER_ANCHOR: &str = "Loop Region Fill";
// const OTHER_ANCHOR_2: &str = "Cue Marker Selected Fill";

// Used to search for raw color class, it has constants and one of them (black) is used for timeline playing position
const RAW_COLOR_ANCHOR: f64 = 0.666333;

// Timeline playing position!
// For 5.2 Beta 1 it's located at com/bitwig/flt/widget/core/timeline/renderer/mH
// method looks like this:
//
// public void kHn(VjN vjN, double d) {
//     YCn yCn;
//     double d2 = this.kHn(d);
//     if (d2 >= (double)((yCn = vjN.kp_()).SWO() - 5L) && d2 <= (double)(yCn.FrR() + 5L)) {
//         vjN.EvR(HyF.kHn); <----- THIIIIIIIIIIIS IS BLACK CONSTANT USAGE!
//         vjN.L1z(vjN.kHn(1L));
//         vjN.ajg(d2, 0.0);
//         vjN.kHn(d2, (double)this.kHn.GXQ());
//         vjN.Q1d();
//     }
// }

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

fn find_method_by_sig(
    class: &Class<'_>,
    sig_start: &str,
    method_name: &str,
) -> Option<(u16, MethodDescription)> {
    let rp = init_refprinter(&class.cp, &class.attrs);

    let method = class.methods.iter().skip(1).next();
    let method = method?;

    let attr = method.attrs.first()?;
    let classfile::attrs::AttrBody::Code((code_1, _code_2)) = &attr.body else {
        return None;
    };
    let bytecode = &code_1.bytecode;

    for (_pos, ix) in &bytecode.0 {
        if let Instr::Invokevirtual(method_id) = &ix {
            let method_descr = find_method_description(&rp, *method_id, None)?;
            if method_descr.signature.starts_with(sig_start) && method_descr.method == method_name {
                return Some((*method_id, method_descr));
            }
        }
    }

    None
}

fn replace_named_color<'a>(
    class: &mut Class<'a>,
    name: &str,
    new_value: ColorComponents,
    named_colors: &mut [NamedColor],
    palette_color_meths: &'a PaletteColorMethods,
    compositing_mode: CompositingMode,
) -> Option<()> {
    if !matches!(
        new_value,
        ColorComponents::Rgbai(..) | ColorComponents::DeltaHsvf(..)
    ) {
        todo!("Only Rgbai and Hsvf supported for the moment");
    }

    if matches!(compositing_mode, CompositingMode::RelativeToBackground) {
        warn!("Relative compositing is not supported yet: {}", name);
        return None;
    }

    let named_color = named_colors
        .iter_mut()
        .find(|color| color.color_name == name)?;

    debug!("### REPLACING {}: {:?}", name, new_value);

    let method_descr_to_find = match compositing_mode {
        CompositingMode::BlendedOnBackground => &palette_color_meths.rgba_i_blended_on_background,
        CompositingMode::RelativeToBackground => &palette_color_meths.hsv_f_relative_to_background,
        CompositingMode::Absolute => &palette_color_meths.rgba_i_absolute,
    };

    let (rgbai_method_id, _rgbai_method_desc) = match find_method_by_sig(
        class,
        &method_descr_to_find.signature,
        &method_descr_to_find.method,
    ) {
        Some(met) => met,
        None => {
            let rgbai_method_desc = &palette_color_meths.rgba_i_absolute;

            let consts = &mut class.cp.0;

            let class_utf_id = consts.len();
            consts.push(Const::Utf8(BStr(rgbai_method_desc.class.as_bytes())));

            let method_utf_id = consts.len();
            consts.push(Const::Utf8(BStr(rgbai_method_desc.method.as_bytes())));

            let sig_utf_id = consts.len();
            consts.push(Const::Utf8(BStr(rgbai_method_desc.signature.as_bytes())));

            let class_id = consts.len();
            consts.push(Const::Class(class_utf_id as u16));

            let name_and_type_id = consts.len();
            consts.push(Const::NameAndType(method_utf_id as u16, sig_utf_id as u16));

            let method_id = consts.len();
            consts.push(Const::Method(class_id as u16, name_and_type_id as u16));

            (method_id as u16, rgbai_method_desc.clone())
        }
    };

    let rp = init_refprinter(&class.cp, &class.attrs);

    let old_desc = palette_color_meths.from_components(&named_color.components);

    let method = class.methods.get_mut(named_color.method_idx)?;

    let attr = method.attrs.first_mut()?;

    let classfile::attrs::AttrBody::Code((code_1, _code_2)) = &mut attr.body else {
        return None;
    };
    if code_1.stack < 7 {
        code_1.stack = 7;
    }
    let bytecode = &mut code_1.bytecode;
    let mut old_bytecode = bytecode.0.drain(..);
    let mut new_bytecode: Vec<(Pos, Instr)> = vec![];
    let mut pos_gen = 0..;

    let mut ready = false;

    while let Some((_, ix)) = old_bytecode.next() {
        new_bytecode.push((Pos(pos_gen.next()?), ix));
        if ready {
            continue;
        }

        let id = match new_bytecode.last()?.1 {
            Instr::Ldc(id) => id as u16,
            Instr::LdcW(id) => id as u16,
            _ => {
                continue;
            }
        };

        let Some(text) = find_utf_ldc(&rp, id as u16) else {
            continue;
        };
        if text == name {
            loop {
                let ix = old_bytecode.next().unwrap();
                if let Instr::Invokevirtual(method_id) = ix.1 {
                    let desc = find_method_description(&rp, method_id, None).unwrap();
                    if desc.signature == old_desc.signature {
                        break;
                    }
                }
            }
            let (ixs_to_push, floats_to_add) = new_value.to_ixs(class.cp.0.len());
            for ix in ixs_to_push {
                new_bytecode.push((Pos(pos_gen.next()?), ix));
            }
            if let Some(floats) = floats_to_add {
                for float in floats {
                    class
                        .cp
                        .0
                        .push(Const::Float(u32::from_be_bytes(float.float.to_be_bytes())));
                }
            }

            // Now invoke correct method instead of old
            new_bytecode.push((Pos(pos_gen.next()?), Instr::Invokevirtual(rgbai_method_id)));
            named_color.components = new_value.clone();
            ready = true;
        }
    }
    drop(old_bytecode);

    bytecode.0 = new_bytecode;

    for attr in &mut code_1.attrs {
        let classfile::attrs::AttrBody::LineNumberTable(table) = &mut attr.body else {
            continue;
        };
        table.clear();
    }

    Some(())
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

fn init_refprinter<'a>(cp: &ConstPool<'a>, attrs: &'a [Attribute<'a>]) -> RefPrinter<'a> {
    let mut bstable = None;
    let mut inner_classes = None;
    for attr in attrs {
        use AttrBody::*;
        match &attr.body {
            BootstrapMethods(v) => bstable = Some(v.as_ref()),
            InnerClasses(v) => inner_classes = Some(v.as_ref()),
            _ => {}
        }
    }

    let rp = RefPrinter::new(true, &cp, bstable, inner_classes);

    rp
}

fn find_method_description(
    rp: &RefPrinter<'_>,
    method_id: u16,
    color_rec_name: Option<&str>,
) -> Option<MethodDescription> {
    let const_line = rp.cpool.get(method_id as usize)?;
    let ConstData::Fmim(FmimTag::Method, c, nat) = const_line.data else {
        return None;
    };

    let class = {
        let const_line = rp.cpool.get(c as usize)?;
        let ConstData::Single(SingleTag::Class, c) = const_line.data else {
            return None;
        };
        let const_line = rp.cpool.get(c as usize)?;
        let ConstData::Utf8(utf_data) = &const_line.data else {
            return None;
        };
        utf_data.s.to_string()
    };

    let const_line = rp.cpool.get(nat as usize)?;
    let ConstData::Nat(met, sig) = const_line.data else {
        return None;
    };

    let method = {
        let const_line = rp.cpool.get(met as usize)?;
        let ConstData::Utf8(utf_data) = &const_line.data else {
            return None;
        };
        utf_data.s.to_string()
    };

    let signature = {
        let const_line = rp.cpool.get(sig as usize)?;
        let ConstData::Utf8(utf_data) = &const_line.data else {
            return None;
        };
        utf_data.s.to_string()
    };

    let signature_kind = if let Some((sig_start, _)) = signature.split_once(")") {
        use MethodSignatureKind::*;
        match sig_start {
            "(Ljava/lang/String;I" => Some(Si),
            "(Ljava/lang/String;III" => Some(Siii),
            "(Ljava/lang/String;IIII" => Some(Siiii),
            "(Ljava/lang/String;FFF" => Some(Sfff),
            "(Ljava/lang/String;Ljava/lang/String;FFF" => Some(SSfff),
            "(FFFF" => Some(Ffff),
            "(DDDD" => Some(Dddd),
            _ => {
                if let Some(color_rec_name) = color_rec_name {
                    if sig_start == &format!("(Ljava/lang/String;L{};FFF", color_rec_name) {
                        Some(SRfff)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    } else {
        None
    };

    Some(MethodDescription {
        class,
        method,
        signature,
        signature_kind,
    })
}

fn find_utf_ldc(rp: &RefPrinter<'_>, id: u16) -> Option<String> {
    let const_line = rp.cpool.get(id as usize)?;
    let ConstData::Single(SingleTag::String, idx) = const_line.data else {
        return None;
    };
    let const_line = rp.cpool.get(idx as usize)?;
    let ConstData::Utf8(utf_data) = &const_line.data else {
        return None;
    };
    return Some(utf_data.s.to_string());
}

fn find_const_name(rp: &RefPrinter<'_>, id: u16) -> Option<String> {
    let const_line = rp.cpool.get(id as usize)?;

    let ConstData::Fmim(FmimTag::Field, _c, nat) = const_line.data else {
        return None;
    };

    let const_line = rp.cpool.get(nat as usize)?;
    let ConstData::Nat(const_name, class_name) = const_line.data else {
        return None;
    };

    let const_line = rp.cpool.get(const_name as usize)?;
    let ConstData::Utf8(utf_data) = &const_line.data else {
        return None;
    };
    let const_name = utf_data.s.to_string();

    let const_line = rp.cpool.get(class_name as usize)?;
    let ConstData::Utf8(utf_data) = &const_line.data else {
        return None;
    };
    let _class_name = utf_data.s.to_string();

    Some(const_name)
}

fn detect_timeline_color_const(class: &Class) -> Option<(u16, u16, String)> {
    let rp = init_refprinter(&class.cp, &class.attrs);

    let method = class.methods.iter().find_map(|method| {
        let ConstData::Utf8(id) = &rp.cpool.get(method.desc as usize)?.data else {
            return None;
        };
        let sig = id.s.to_string();
        let sig_is_good = sig.starts_with("(Lcom/bitwig/graphics/") && sig.ends_with(";D)V");

        if sig_is_good {
            Some(method)
        } else {
            None
        }
    })?;

    let Some(attr) = method.attrs.first() else {
        return None;
    };
    let AttrBody::Code((code_1, _)) = &attr.body else {
        return None;
    };

    let bytecode = &code_1.bytecode;

    let mut count_of_5l = 0;
    let mut has_dcmpg = false;
    let mut has_ifgt = false;

    let mut ifgt_idx = 0;

    for (idx, (_, ix)) in bytecode.0.iter().enumerate() {
        match ix {
            Instr::Ldc2W(ind) => {
                let ConstData::Prim(PrimTag::Long, b) = &rp.cpool.get(*ind as usize).unwrap().data
                else {
                    continue;
                };
                if b == "5L" {
                    count_of_5l += 1;
                }
            }
            Instr::Dcmpg => {
                if count_of_5l == 2 {
                    has_dcmpg = true;
                }
            }
            Instr::Ifgt(..) => {
                if has_dcmpg {
                    has_ifgt = true;
                    ifgt_idx = idx;
                    break;
                }
            }
            _ => {}
        }
    }

    if !has_ifgt {
        return None;
    }

    let get_static_ix_idx = ifgt_idx + 2;
    let Instr::Getstatic(fmim_idx) = &bytecode.0.get(get_static_ix_idx)?.1 else {
        return None;
    };
    let ConstData::Fmim(FmimTag::Field, _class_cp_idx, fld_id) =
        &rp.cpool.get(*fmim_idx as usize)?.data
    else {
        return None;
    };
    let ConstData::Nat(field_cp_idx, field_type_cp_idx) = &rp.cpool.get(*fld_id as usize)?.data
    else {
        return None;
    };
    let ConstData::Utf8(utf) = &rp.cpool.get(*field_cp_idx as usize)?.data else {
        return None;
    };
    let cnst_name = utf.s.to_string();
    Some((*field_type_cp_idx, *fmim_idx, cnst_name))
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

#[derive(Debug)]
enum UsefulFileType {
    MainPalette,
    RawColor,
    Init,
    CrashReport,
    TimelineColorCnst {
        field_type_cp_idx: u16,
        fmim_idx: u16,
        cnst_name: String,
    },
}

fn is_useful_file(class: &Class) -> Option<UsefulFileType> {
    if let Some(mtch) =
        has_any_string_in_constant_pool(class, &[PALETTE_ANCHOR, INIT_ANCHOR, CRASH_REPORT_ANCHOR])
    {
        let useful_file_type = match mtch {
            PALETTE_ANCHOR => UsefulFileType::MainPalette,
            INIT_ANCHOR => UsefulFileType::Init,
            CRASH_REPORT_ANCHOR => UsefulFileType::CrashReport,
            _ => return None,
        };
        return Some(useful_file_type);
    }

    if let Some(_) = has_any_double_in_constant_pool(class, &[RAW_COLOR_ANCHOR]) {
        return Some(UsefulFileType::RawColor);
    }

    if let Some((field_type_cp_idx, fmim_idx, cnst_name)) = detect_timeline_color_const(class) {
        return Some(UsefulFileType::TimelineColorCnst {
            field_type_cp_idx,
            fmim_idx,
            cnst_name,
        });
    }

    return None;
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

fn has_any_string_in_constant_pool<'a>(class: &Class, strings: &[&'a str]) -> Option<&'a str> {
    for entry in &class.cp.0 {
        if let classfile::cpool::Const::Utf8(txt) = entry {
            let parsed_string = String::from_utf8_lossy(txt.0);
            if let Some(found) = strings.iter().find(|pattern| **pattern == parsed_string) {
                return Some(found);
            }
        }
    }

    None
}

fn has_any_double_in_constant_pool<'a>(class: &Class, doubles: &[f64]) -> Option<f64> {
    for entry in &class.cp.0 {
        if let classfile::cpool::Const::Double(double_as_u64) = entry {
            let bytes = double_as_u64.to_be_bytes();
            let double_as_f64 = f64::from_be_bytes(bytes);
            if let Some(found) = doubles.iter().find(|dbl| **dbl == double_as_f64) {
                return Some(*found);
            }
        }
    }

    None
}
