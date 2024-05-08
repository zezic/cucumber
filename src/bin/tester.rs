use std::{collections::HashMap, env, fmt::Debug, fs, io::Read};

use colorsys::{ColorTransform, Rgb, SaturationInSpace};
// use indicatif::ProgressBar;
use krakatau2::{
    lib::{
        classfile::{
            self,
            attrs::{AttrBody, Attribute},
            code::{Bytecode, Instr},
            cpool::ConstPool,
            parse::Class,
        },
        disassemble::refprinter::{ConstData, FmimTag, RefPrinter, SingleTag},
        parse_utf8, ParserOptions,
    },
    zip,
};

// Will search constant pool for that (inside Utf8 entry)
// Contain most of the colors and methods to set them
const PALETTE_ANCHOR: &str = "Device Tint Future";
// Contain time-bomb initialization around constant 5000
const INIT_ANCHOR: &str = "Apply Device Remote Control Changes To All Devices";
// Other color anchor
// const OTHER_ANCHOR: &str = "Loop Region Fill";
// const OTHER_ANCHOR_2: &str = "Cue Marker Selected Fill";

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

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let input_jar = &args[1];

    let file = fs::File::open(input_jar)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let file_names = zip.file_names().map(Into::into).collect::<Vec<String>>();
    const PARSER_OPTIONS: ParserOptions = ParserOptions {
        no_short_code_attr: true,
    };

    let mut palette_color_meths = None;
    let mut raw_color_meths = None;

    let mut data = Vec::new();

    // let progress_bar = ProgressBar::new(file_names.len() as u64);
    for file_name in &file_names {
        let mut file = zip.by_name(file_name).unwrap();

        data.clear();
        file.read_to_end(&mut data)?;

        let Ok(class) = classfile::parse(&data, PARSER_OPTIONS) else {
            continue;
        };

        if let Some(useful_file_type) = is_useful_file(&class) {
            match useful_file_type {
                UsefulFileType::MainPalette => {
                    println!("Found main palette: {}", file_name);
                    if let Some(methods) = extract_palette_color_methods(&class) {
                        // println!("{:#?}", methods);
                        palette_color_meths = Some(methods);
                    }
                }
                UsefulFileType::Init => {
                    println!("Found init: {}", file_name);
                }
                UsefulFileType::RawColor => {
                    println!("Found raw color: {}", file_name);
                    if let Some(methods) = extract_raw_color_methods(&class) {
                        println!("{:#?}", methods);
                        raw_color_meths = Some(methods);
                    }
                }
            }
        }
        // progress_bar.inc(1);
        drop(file);
    }
    // progress_bar.finish();
    println!("------------");

    if let Some(palette_color_meths) = palette_color_meths {
        for file_name in &file_names {
            let mut file = zip.by_name(&file_name).unwrap();

            data.clear();
            file.read_to_end(&mut data)?;

            let Ok(class) = classfile::parse(&data, PARSER_OPTIONS) else {
                continue;
            };

            scan_for_named_color_defs(&class, &palette_color_meths, &file_name);
            drop(file);
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MethodDescription {
    class: String,
    method: String,
    signature: String,
    signature_kind: Option<MethodSignatureKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MethodSignatureKind {
    Si,
    Siii,
    Siiii,
    Sfff,
    SRfff, // R - reference to other, already defined color
    SSfff,
}

trait IxToInt {
    fn to_int(&self) -> u8;
}

trait IxToFloat {
    fn to_float(&self, refprinter: &RefPrinter) -> f32;
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
        match self {
            Instr::Fconst0 => 0.0,
            Instr::Fconst1 => 1.0,
            Instr::Fconst2 => 2.0,
            Instr::Dconst0 => 0.0,
            Instr::Dconst1 => 1.0,
            Instr::Ldc(ind) => {
                let data = refprinter.cpool.get(*ind as usize).unwrap();
                match &data.data {
                    ConstData::Prim(_prim_tag, text) => {
                        match text.trim_end_matches("f").parse::<f32>() {
                            Ok(val) => val,
                            Err(err) => {
                                panic!("err parse f32 [{}]: {}", text, err);
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
        match self {
            MethodSignatureKind::Si => ColorComponents::Grayscale(int(1)),
            MethodSignatureKind::Siii => ColorComponents::Rgbi(int(3), int(2), int(1)),
            MethodSignatureKind::Siiii => ColorComponents::Rgbai(int(4), int(3), int(2), int(1)),
            MethodSignatureKind::Sfff => ColorComponents::Rgbf(float(3), float(2), float(1)),
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
        }
    }
}

enum ColorComponents {
    Grayscale(u8),
    Rgbi(u8, u8, u8),
    Rgbai(u8, u8, u8, u8),
    Rgbf(f32, f32, f32),
    RefAndAdjust(String, f32, f32, f32),
    StringAndAdjust(String, f32, f32, f32),
}

impl ColorComponents {
    fn to_rgb(&self, known_colors: &HashMap<String, ColorComponents>) -> (u8, u8, u8) {
        match self {
            ColorComponents::Grayscale(v) => (*v, *v, *v),
            ColorComponents::Rgbi(r, g, b) => (*r, *g, *b),
            ColorComponents::Rgbai(r, g, b, _a) => (*r, *g, *b),
            ColorComponents::Rgbf(r, g, b) => {
                ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
            }
            ColorComponents::RefAndAdjust(_, _, _, _) => todo!(),
            ColorComponents::StringAndAdjust(ref_name, h, s, v) => {
                let Some(known) = known_colors.get(ref_name) else {
                    panic!("Unknown color ref: {}", ref_name);
                };
                let (r, g, b) = known.to_rgb(&known_colors);
                let mut rgb = Rgb::from((r, g, b));
                rgb.adjust_hue(*h as f64);
                rgb.saturate(SaturationInSpace::Hsl(*s as f64 * 100.));
                rgb.lighten(*v as f64 * 100.);
                rgb.into()
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
            x => {
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
        println!("!!!!!!>> {:?}", const_line.data);
        return None;
    };
    let const_line = rp.cpool.get(idx as usize)?;
    let ConstData::Utf8(utf_data) = &const_line.data else {
        return None;
    };
    return Some(utf_data.s.to_string());
}

fn scan_for_named_color_defs(
    class: &Class,
    palette_color_meths: &PaletteColorMethods,
    filename: &str,
) {
    let mut known_colors = HashMap::new();
    let rp = init_refprinter(&class.cp, &class.attrs);

    let all_meths = palette_color_meths.all();

    for method in &class.methods {
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

            for meth in &all_meths {
                if method_descr == **meth {
                    if let Some(sig_kind) = &meth.signature_kind {
                        let offset = sig_kind.color_name_ix_offset();
                        let Some((_, ix)) = bytecode.0.get(idx - offset) else {
                            println!("{}: offset out of bounds", filename);
                            continue;
                        };
                        match ix {
                            Instr::Ldc(id) => {
                                let text = find_utf_ldc(&rp, *id as u16);
                                let components =
                                    sig_kind.extract_color_components(idx, bytecode, &rp);
                                let (r, g, b) = components.to_rgb(&known_colors);
                                use colored::Colorize;

                                // If not in-place color name defined, then it's a method call inside other delegate method
                                // so it's not interesting to us (I guess?).
                                if let Some(color_name) = &text {
                                    let debug_line = if (r as u16 + g as u16 + b as u16) > 384 {
                                        format!("{}", color_name).black().on_truecolor(r, g, b)
                                    } else {
                                        format!("{}", color_name).on_truecolor(r, g, b)
                                    };
                                    println!("{}", debug_line);
                                    known_colors.insert(color_name.clone(), components);
                                }
                            }
                            _other => {
                                // println!("{}: {:?}", filename, other);
                            }
                        }
                    } else {
                        println!("No signature kind prepared :(");
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum UsefulFileType {
    MainPalette,
    RawColor,
    Init,
}

fn is_useful_file(class: &Class) -> Option<UsefulFileType> {
    if let Some(mtch) = has_any_string_in_constant_pool(class, &[PALETTE_ANCHOR, INIT_ANCHOR]) {
        let useful_file_type = match mtch {
            PALETTE_ANCHOR => UsefulFileType::MainPalette,
            INIT_ANCHOR => UsefulFileType::Init,
            _ => return None,
        };
        return Some(useful_file_type);
    }

    if let Some(float) = has_any_double_in_constant_pool(class, &[0.666333]) {
        return if float == 0.666333 {
            Some(UsefulFileType::RawColor)
        } else {
            None
        };
    }
    return None;
}

#[derive(Debug)]
struct PaletteColorMethods {
    grayscale_i: MethodDescription,
    rgb_i: MethodDescription,
    rgba_i: MethodDescription,
    rgb_f: MethodDescription,
    ref_hsv_f: MethodDescription,
    name_hsv_f: MethodDescription,
}

#[derive(Debug)]
struct RawColorMethods {
    rgb_i: MethodDescription,
    grayscale_i: MethodDescription,
    rgb_f: MethodDescription,
    rgba_f: MethodDescription,
    rgb_d: MethodDescription,
}

impl PaletteColorMethods {
    fn all(&self) -> Vec<&MethodDescription> {
        vec![
            &self.grayscale_i,
            &self.rgb_i,
            &self.rgba_i,
            &self.rgb_f,
            &self.ref_hsv_f,
            &self.name_hsv_f,
        ]
    }
}

fn extract_raw_color_methods(class: &Class) -> Option<RawColorMethods> {
    println!("Searching raw color methods");

    // let rp = init_refprinter(&class.cp, &class.attrs);

    let class_name = class.cp.clsutf(class.this).and_then(parse_utf8)?;
    // println!("Class >>>>> {}", class_name);

    for method in &class.methods {
        println!("METH IDX: {}", method.name);
        let Some(meth_name) = class.cp.utf8(method.name).and_then(parse_utf8) else {
            continue;
        };
        println!("METH NAME: {}", meth_name);
        let Some(attr) = method.attrs.first() else {
            continue;
        };
        let AttrBody::Code((code_1, _)) = &attr.body else {
            continue;
        };
        for (_, ix) in &code_1.bytecode.0 {
            println!("IX: {:?}", ix);
        }
        println!("---");
    }

    todo!();

    None
}

fn extract_palette_color_methods(class: &Class) -> Option<PaletteColorMethods> {
    // println!("Searching palette color methods");

    let rp = init_refprinter(&class.cp, &class.attrs);

    let class_name = class.cp.clsutf(class.this).and_then(parse_utf8)?;
    // println!("Class >>>>> {}", class_name);

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

    let find_method = |signature_start: &str, color_rec_name: Option<&str>| {
        let mut invokes = invokes.clone();
        invokes.find_map(|method_id| {
            let method_descr = find_method_description(&rp, *method_id, color_rec_name)?;
            if method_descr.signature.starts_with(signature_start) {
                Some(method_descr)
            } else {
                None
            }
        })
    };

    let grayscale_i = find_method("(Ljava/lang/String;I)", None)?;
    let color_record_class_name = grayscale_i
        .signature
        .split_once("I)L")
        .map(|(_, suffix)| suffix.strip_suffix(";"))
        .flatten()?;
    let rgb_i = find_method("(Ljava/lang/String;III)", Some(color_record_class_name))?;
    let rgba_i = find_method("(Ljava/lang/String;IIII)", Some(color_record_class_name))?;
    let rgb_f = find_method("(Ljava/lang/String;FFF)", Some(color_record_class_name))?;
    let ref_hsv_f = find_method(
        &format!("(Ljava/lang/String;L{};FFF)", color_record_class_name),
        Some(color_record_class_name),
    )?;
    let name_hsv_f = find_method(
        "(Ljava/lang/String;Ljava/lang/String;FFF)",
        Some(color_record_class_name),
    )?;

    Some(PaletteColorMethods {
        grayscale_i,
        rgb_i,
        rgba_i,
        rgb_f,
        ref_hsv_f,
        name_hsv_f,
    })
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
