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
        disassemble::refprinter::{ConstData, FmimTag, PrimTag, RefPrinter, SingleTag},
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

fn main() {
    let _general_goodies = extract_general_goodies();
    // println!("General goodies: {:#?}", general_goodies);
}

fn extract_general_goodies() -> anyhow::Result<GeneralGoodies> {
    let args: Vec<String> = env::args().collect();
    let input_jar = &args[1];

    let file = fs::File::open(input_jar)?;
    let mut zip = zip::ZipArchive::new(file)?;


    let file_names = zip.file_names().map(Into::into).collect::<Vec<String>>();
    const PARSER_OPTIONS: ParserOptions = ParserOptions {
        no_short_code_attr: true,
    };

    let mut palette_color_meths = None;
    let mut raw_color_goodies = None;
    let mut timeline_color_ref = None;

    let mut data = Vec::new();

    // let progress_bar = ProgressBar::new(file_names.len() as u64);
    let mut init_class_name = None;
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
                    init_class_name = Some(file_name.clone());
                }
                UsefulFileType::RawColor => {
                    println!("Found raw color: {}", file_name);
                    if let Some(goodies) = extract_raw_color_goodies(&class) {
                        raw_color_goodies = Some(goodies);
                    }
                }
                UsefulFileType::TimelineColorCnst{ cpool_idx, cnst_name } => {
                    println!("Found timeline color const: {}", file_name);
                    timeline_color_ref = Some(TimelineColorReference {
                        class_name: file_name.clone(),
                        const_name: cnst_name,
                        cpool_idx,
                    });
                },
            }
        }
        // progress_bar.inc(1);
        drop(file);
    }
    // progress_bar.finish();
    println!("------------");

    let mut all_named_colors = Vec::new();

    let mut known_colors = HashMap::new();

    if let Some(palette_color_meths) = &palette_color_meths {
        for file_name in &file_names {
            let mut file = zip.by_name(&file_name).unwrap();

            data.clear();
            file.read_to_end(&mut data)?;

            let Ok(class) = classfile::parse(&data, PARSER_OPTIONS) else {
                continue;
            };

            let found = scan_for_named_color_defs(&class, &palette_color_meths, &file_name, &mut known_colors);
            all_named_colors.extend(found);
            drop(file);
        }
    }

    for named_color in &all_named_colors {
        debug_print_color(&named_color.class_name, &named_color.color_name, &named_color.components, &known_colors);
    }

    if let Some(raw_color_goodies) = &raw_color_goodies {
        for cnst in &raw_color_goodies.constants.consts {
            debug_print_color(&cnst.class_name, &cnst.const_name, &cnst.color_comps, &known_colors);
        }
    }

    Ok(GeneralGoodies {
        init_class: init_class_name.unwrap(),
        named_colors: all_named_colors,
        palette_color_methods: palette_color_meths.unwrap(),
        raw_colors: raw_color_goodies.unwrap(),
        timeline_color_ref: timeline_color_ref.unwrap()
    })
}

#[derive(Debug)]
struct TimelineColorReference {
    class_name: String,
    const_name: String,
    cpool_idx: usize,
}

#[derive(Debug)]
struct GeneralGoodies {
    init_class: String,
    named_colors: Vec<NamedColor>,
    palette_color_methods: PaletteColorMethods,
    raw_colors: RawColorGoodies,
    timeline_color_ref: TimelineColorReference,
}

#[derive(Debug)]
struct NamedColor {
    class_name: String,
    color_name: String,
    components: ColorComponents,
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
    Ffff,
    Dddd,
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
            MethodSignatureKind::Ffff => {
                ColorComponents::Rgbaf(float(4), float(3), float(2), float(1))
            }
            MethodSignatureKind::Dddd => {
                ColorComponents::Rgbad(double(4), double(3), double(2), double(1))
            }
        }
    }
}

#[derive(Clone, Debug)]
enum ColorComponents {
    Grayscale(u8),
    Rgbi(u8, u8, u8),
    Rgbai(u8, u8, u8, u8),
    Rgbf(f32, f32, f32),
    Rgbaf(f32, f32, f32, f32),
    Rgbad(f64, f64, f64, f64),
    #[allow(dead_code)]
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
            ColorComponents::Rgbaf(r, g, b, _a) => {
                ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
            }
            ColorComponents::Rgbad(r, g, b, _a) => {
                ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
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

fn detect_timeline_color_const(
    class: &Class,
) -> Option<(usize, String)> {
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
                let ConstData::Prim(PrimTag::Long, b) = &rp.cpool.get(*ind as usize).unwrap().data else {
                    continue;
                };
                if b == "5L" {
                    count_of_5l += 1;
                }
            },
            Instr::Dcmpg => {
                if count_of_5l == 2 {
                    has_dcmpg = true;
                }
            },
            Instr::Ifgt(..) => {
                if has_dcmpg {
                    has_ifgt = true;
                    ifgt_idx = idx;
                    break;
                }
            },
            _ => {}
        }
    }

    if !has_ifgt {
        return None;
    }

    let get_static_ix_idx = ifgt_idx + 2;
    let Instr::Getstatic(id) = &bytecode.0.get(get_static_ix_idx)?.1 else {
        return None;
    };
    let ConstData::Fmim(FmimTag::Field, _cls_id, fld_id) = &rp.cpool.get(*id as usize)?.data else {
        return None;
    };
    let ConstData::Nat(field_cp_idx, _field_type_cp_idx) = &rp.cpool.get(*fld_id as usize)?.data else {
        return None;
    };
    let ConstData::Utf8(utf) = &rp.cpool.get(*field_cp_idx as usize)?.data else {
        return None;
    };
    let cnst_name = utf.s.to_string();
    Some((*field_cp_idx as usize, cnst_name))
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

                                // If not in-place color name defined, then it's a method call inside other delegate method
                                // so it's not interesting to us (I guess?).
                                if let Some(color_name) = &text {
                                    found.push(NamedColor {
                                        class_name: class_name.clone(),
                                        color_name: color_name.clone(),
                                        components: components.clone(),
                                    });
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

    found
}

fn debug_print_color(
    class_name: &str,
    color_name: &str,
    components: &ColorComponents,
    known_colors: &HashMap<String, ColorComponents>,
) {
    let (r, g, b) = components.to_rgb(&known_colors);
    use colored::Colorize;
    let debug_line = if (r as u16 + g as u16 + b as u16) > 384 {
        format!("{}", color_name).black().on_truecolor(r, g, b)
    } else {
        format!("{}", color_name).on_truecolor(r, g, b)
    };
    println!("{} ({})", debug_line, class_name);
}

#[derive(Debug)]
enum UsefulFileType {
    MainPalette,
    RawColor,
    Init,
    TimelineColorCnst {
        cpool_idx: usize,
        cnst_name: String,
    },
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

    if let Some(_) = has_any_double_in_constant_pool(class, &[RAW_COLOR_ANCHOR]) {
        return Some(UsefulFileType::RawColor);
    }

    if let Some((cpool_idx, cnst_name)) = detect_timeline_color_const(class) {
        return Some(UsefulFileType::TimelineColorCnst { cpool_idx, cnst_name })
    }

    return None;
}

// Color methods and defined static colors (contain important black color)
#[derive(Debug)]
struct RawColorGoodies {
    methods: RawColorMethods,
    constants: RawColorConstants,
}

// Color methods and defined static colors (contain important black color)
#[derive(Debug)]
struct RawColorMethods {
    // rgb_i: MethodDescription,
    // grayscale_i: MethodDescription,
    // rgb_f: MethodDescription,
    rgba_f: MethodDescription,
    // rgb_d: MethodDescription,
    rgba_d: MethodDescription,
}

impl RawColorMethods {
    fn all(&self) -> Vec<&MethodDescription> {
        vec![&self.rgba_f, &self.rgba_d]
    }
}

#[derive(Debug)]
struct RawColorConstants {
    consts: Vec<RawColorConst>,
}

#[derive(Debug)]
struct RawColorConst {
    class_name: String,
    const_name: String,
    color_comps: ColorComponents,
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

fn extract_raw_color_goodies(class: &Class) -> Option<RawColorGoodies> {
    let rp = init_refprinter(&class.cp, &class.attrs);

    let class_name = class.cp.clsutf(class.this).and_then(parse_utf8)?;

    let mut rgbaf_desc = None;
    let mut rgbad_desc = None;

    // At first, find all popular constructors
    for method in &class.methods {
        // println!("METH IDX: {}", method.name);
        let Some(meth_name) = class.cp.utf8(method.name).and_then(parse_utf8) else {
            continue;
        };
        // println!("METH: {}", meth_name);
        // println!("METH NAME: {}", meth_name);
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
        // println!("METH IDX: {}", method.name);
        let Some(meth_name) = class.cp.utf8(method.name).and_then(parse_utf8) else {
            continue;
        };
        // println!("METH: {}", meth_name);
        // println!("METH NAME: {}", meth_name);
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
                // println!("{:?}", desc);
                // let const_line = rp.cpool.get(*method_id as usize).unwrap();
                // let ConstData::Utf8(utf_data) = &const_line.data else {
                //     panic!("Can't find method desc");
                // };
                // let sig = utf_data.s.to_string();

                // println!("{} {:?} {:?}", pos, ix, const_line);
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
    // println!("Searching palette color methods");

    let rp = init_refprinter(&class.cp, &class.attrs);

    let _class_name = class.cp.clsutf(class.this).and_then(parse_utf8)?;
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
