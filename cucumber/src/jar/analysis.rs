use super::{extract_raw_color_goodies, scan_for_named_color_defs};
use crate::{
    jar::{
        debug::debug_print_color,
        goodies::{GeneralGoodies, MethodDescription, MethodSignatureKind, PaletteColorMethods},
        legacy::TimelineColorReference,
    },
    types::{Stage, StageProgress, ThemeProcessingEvent},
};
use krakatau2::lib::{
    classfile::{
        self,
        attrs::{AttrBody, Attribute},
        code::{Instr, Pos},
        cpool::ConstPool,
        parse::Class,
    },
    disassemble::refprinter::{ConstData, FmimTag, PrimTag, RefPrinter, SingleTag},
    parse_utf8, ParserOptions,
};
use krakatau2::zip::ZipArchive;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use tracing::debug;

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

pub fn extract_general_goodies<R: std::io::Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    mut report_progress: impl FnMut(ThemeProcessingEvent),
) -> anyhow::Result<GeneralGoodies> {
    const PARSER_OPTIONS: ParserOptions = ParserOptions {
        no_short_code_attr: true,
    };

    report_progress(Stage::LoadingFileNames.into());

    let file_names = zip.file_names().map(Into::into).collect::<Vec<String>>();

    let mut palette_color_meths = None;
    let mut init_class_name = None;
    let mut raw_color_goodies = None;
    let mut timeline_color_ref = None;
    let mut release_metadata = None;

    let mut named_color_getter_1 = None;
    let mut deferred_named_color_getter_1_extraction = None;

    let mut named_color_getter_invocations = Vec::new();

    let mut data = Vec::new();

    report_progress(ThemeProcessingEvent {
        stage: Stage::ScanningClasses,
        progress: StageProgress::Percentage(0.0),
    });

    for (idx, file_name) in file_names.iter().enumerate() {
        let mut file = zip.by_name(file_name).unwrap();
        data.clear();
        file.read_to_end(&mut data)?;
        let Ok(class) = classfile::parse(&data, PARSER_OPTIONS) else {
            continue;
        };

        if let Some(useful_file_type) = is_useful_file(&class) {
            match useful_file_type {
                UsefulFileType::MainPalette => {
                    debug!("Found main palette: {}", file_name);
                    if let Some(methods) = extract_palette_color_methods(&class) {
                        // debug!("{:#?}", methods);
                        palette_color_meths = Some(methods);
                    }
                }
                UsefulFileType::Init => {
                    debug!("Found init: {}", file_name);
                    init_class_name = Some(file_name.clone());
                }
                UsefulFileType::RawColor => {
                    debug!("Found raw color: {}", file_name);
                    if let Some(goodies) = extract_raw_color_goodies(&class) {
                        raw_color_goodies = Some(goodies);
                    }
                }
                UsefulFileType::CrashReport => {
                    debug!("Found crash report: {}", file_name);
                    if let Some(metadata) = extract_release_metadata(&class) {
                        release_metadata = Some(metadata);
                    }
                }
                UsefulFileType::TimelineColorCnst {
                    field_type_cp_idx,
                    fmim_idx: class_cp_idx,
                    cnst_name,
                } => {
                    debug!("Found timeline color const: {}", file_name);
                    timeline_color_ref = Some(TimelineColorReference {
                        class_filename: file_name.clone(),
                        const_name: cnst_name,
                        field_type_cp_idx,
                        fmim_idx: class_cp_idx,
                    });
                }
                UsefulFileType::NamedColorGetter1 => {
                    debug!("Found named color getter 1: {}", file_name);
                    if let Some(raw_color_goodies) = &raw_color_goodies {
                        if let Some(method_description) = extract_named_color_getter_1(
                            &class,
                            &raw_color_goodies.methods.rgba_f.class,
                        ) {
                            named_color_getter_1 = Some(method_description);
                        }
                    } else {
                        deferred_named_color_getter_1_extraction = Some(file_name.clone());
                    }
                }
            }
        }
        drop(file);

        // Report progress every 300 files, which is about 100 reports per typical 30k bloated JAR
        if idx % 300 == 0 {
            let progress = idx as f32 / file_names.len() as f32;
            report_progress(ThemeProcessingEvent {
                stage: Stage::ScanningClasses,
                progress: StageProgress::Percentage(progress),
            });
        }
    }
    report_progress(ThemeProcessingEvent {
        stage: Stage::ScanningClasses,
        progress: StageProgress::Done,
    });
    debug!("------------");

    if let Some(deferred_named_color_getter_1) = deferred_named_color_getter_1_extraction {
        let mut file = zip.by_name(&deferred_named_color_getter_1).unwrap();

        data.clear();
        file.read_to_end(&mut data)?;

        let class = classfile::parse(&data, PARSER_OPTIONS).unwrap();

        if let Some(method_description) = extract_named_color_getter_1(
            &class,
            &raw_color_goodies.as_ref().unwrap().methods.rgba_f.class,
        ) {
            named_color_getter_1 = Some(method_description);
        }
    }

    let mut all_named_colors = Vec::new();

    let mut known_colors = HashMap::new();

    if let Some(palette_color_meths) = &palette_color_meths {
        report_progress(ThemeProcessingEvent {
            stage: Stage::SearchingColorDefinitions,
            progress: StageProgress::Percentage(0.0),
        });
        for (idx, file_name) in file_names.iter().enumerate() {
            let mut file = zip.by_name(&file_name).unwrap();

            data.clear();
            file.read_to_end(&mut data)?;

            let Ok(class) = classfile::parse(&data, PARSER_OPTIONS) else {
                continue;
            };

            let found = scan_for_named_color_defs(
                &class,
                &palette_color_meths,
                &file_name,
                &mut known_colors,
            );
            all_named_colors.extend(found);

            if let Some(named_color_getter) = &named_color_getter_1 {
                let invocations = find_named_color_getter_1_invocations(&class, named_color_getter);
                named_color_getter_invocations.extend(invocations);
            }

            drop(file);

            // Report progress every 300 files, which is about 100 reports per typical 30k bloated JAR
            if idx % 300 == 0 {
                let progress = idx as f32 / file_names.len() as f32;
                report_progress(ThemeProcessingEvent {
                    stage: Stage::SearchingColorDefinitions,
                    progress: StageProgress::Percentage(progress),
                });
            }
        }
        report_progress(ThemeProcessingEvent {
            stage: Stage::SearchingColorDefinitions,
            progress: StageProgress::Done,
        });
    }

    for named_color in &all_named_colors {
        debug_print_color(
            &named_color.class_name,
            &named_color.color_name,
            &named_color.components,
            &known_colors,
        );
    }

    if let Some(raw_color_goodies) = &raw_color_goodies {
        for cnst in &raw_color_goodies.constants.consts {
            debug_print_color(
                &cnst.class_name,
                &cnst.const_name,
                &cnst.color_comps,
                &known_colors,
            );
        }
    }

    Ok(GeneralGoodies {
        init_class: init_class_name.unwrap(),
        named_colors: all_named_colors,
        palette_color_methods: palette_color_meths.unwrap(),
        raw_colors: raw_color_goodies.unwrap(),
        timeline_color_ref,
        release_metadata: release_metadata.unwrap_or_default(),
        named_color_getter_1: named_color_getter_1.unwrap(),
        named_color_getter_invocations,
    })
}

#[derive(Debug)]
enum UsefulFileType {
    MainPalette,
    RawColor,
    Init,
    CrashReport,
    NamedColorGetter1,
    TimelineColorCnst {
        field_type_cp_idx: u16,
        fmim_idx: u16,
        cnst_name: String,
    },
}

fn is_useful_file(class: &Class) -> Option<UsefulFileType> {
    if let Some(mtch) = has_any_string_in_constant_pool(
        class,
        &[
            PALETTE_ANCHOR,
            INIT_ANCHOR,
            CRASH_REPORT_ANCHOR,
            NAMED_COLOR_GETTER_1_ANCHOR,
        ],
    ) {
        let useful_file_type = match mtch {
            PALETTE_ANCHOR => UsefulFileType::MainPalette,
            INIT_ANCHOR => UsefulFileType::Init,
            CRASH_REPORT_ANCHOR => UsefulFileType::CrashReport,
            NAMED_COLOR_GETTER_1_ANCHOR => UsefulFileType::NamedColorGetter1,
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

pub fn init_refprinter<'a>(cp: &ConstPool<'a>, attrs: &'a [Attribute<'a>]) -> RefPrinter<'a> {
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

pub fn find_method_by_sig(
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

pub fn find_method_description(
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

pub fn find_utf_ldc(rp: &RefPrinter<'_>, id: u16) -> Option<String> {
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

pub fn find_const_name(rp: &RefPrinter<'_>, id: u16) -> Option<String> {
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

fn extract_named_color_getter_1(class: &Class, raw_color_class: &str) -> Option<MethodDescription> {
    let sig_start = format!("(Ljava/lang/String;)L{};", raw_color_class);
    for field in &class.methods {
        let descriptor = class.cp.utf8(field.desc).and_then(parse_utf8)?;
        if descriptor.starts_with(&sig_start) {
            let class_name = class.cp.clsutf(class.this).and_then(parse_utf8)?;
            let method = class.cp.utf8(field.name).and_then(parse_utf8)?;

            return Some(MethodDescription {
                class: class_name,
                method,
                signature: descriptor,
                signature_kind: None,
            });
        }
    }
    None
}

fn extract_palette_color_methods(class: &Class) -> Option<PaletteColorMethods> {
    let rp = init_refprinter(&class.cp, &class.attrs);

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

    let grayscale_i = find_method("(Ljava/lang/String;I)", None, None, &invokes, &rp)?;
    let color_record_class_name = grayscale_i
        .signature
        .split_once("I)L")
        .map(|(_, suffix)| suffix.strip_suffix(";"))
        .flatten()?;
    let rgb_i = find_method(
        "(Ljava/lang/String;III)",
        Some(color_record_class_name),
        None,
        &invokes,
        &rp,
    )?;
    let rgba_i_absolute = find_method(
        "(Ljava/lang/String;IIII)",
        Some(color_record_class_name),
        None,
        &invokes,
        &rp,
    )?;
    // TODO: search this method not by position, but by difference against rgba_i_absolute
    let rgba_i_blended_on_background = find_method(
        "(Ljava/lang/String;IIII)",
        Some(color_record_class_name),
        Some(1),
        &invokes,
        &rp,
    )?;
    let hsv_f_relative_to_background = find_method(
        "(Ljava/lang/String;FFF)",
        Some(color_record_class_name),
        None,
        &invokes,
        &rp,
    )?;
    let ref_hsv_f = find_method(
        &format!("(Ljava/lang/String;L{};FFF)", color_record_class_name),
        Some(color_record_class_name),
        None,
        &invokes,
        &rp,
    )?;
    let name_hsv_f = find_method(
        "(Ljava/lang/String;Ljava/lang/String;FFF)",
        Some(color_record_class_name),
        None,
        &invokes,
        &rp,
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

fn find_method<'a, T>(
    signature_start: &str,
    color_rec_name: Option<&str>,
    skip: Option<usize>,
    invokes: &'a T,
    rp: &RefPrinter<'_>,
) -> Option<MethodDescription>
where
    T: Iterator<Item = &'a u16> + Clone,
{
    let invokes = invokes.clone();
    invokes
        .filter_map(|method_id| {
            let method_descr = find_method_description(rp, *method_id, color_rec_name)?;
            if method_descr.signature.starts_with(signature_start) {
                Some(method_descr)
            } else {
                None
            }
        })
        .skip(skip.unwrap_or_default())
        .next()
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

#[derive(Debug)]
pub struct NamedColorGetterInvocation {
    /// Class, where method was invoked
    pub class: String,
    /// Method, inside which the invocation was found
    pub method: String,
    /// Position of the ldc instruction used to load the color name
    pub ldc_pos: Pos,
}

/// Find all invocations of the named color getter method.
///
/// Returns a vector of tuples containing the color name and the invocation details.
fn find_named_color_getter_1_invocations(
    class: &Class,
    named_color_getter: &MethodDescription,
) -> Vec<(String, NamedColorGetterInvocation)> {
    let rp = init_refprinter(&class.cp, &class.attrs);
    let mut results = Vec::new();

    let class_name = class
        .cp
        .clsutf(class.this)
        .and_then(parse_utf8)
        .unwrap_or_default();

    for method in &class.methods {
        let method_name = class
            .cp
            .utf8(method.name)
            .and_then(parse_utf8)
            .unwrap_or_default();

        let Some(attr) = method.attrs.first() else {
            continue;
        };
        let AttrBody::Code((code_1, _)) = &attr.body else {
            continue;
        };

        let bytecode = &code_1.bytecode;

        // First, find all invocations of the target method
        let mut invocation_positions = Vec::new();
        for (pos, instr) in &bytecode.0 {
            if let Instr::Invokevirtual(method_id) = instr {
                if let Some(method_descr) = find_method_description(&rp, *method_id, None) {
                    if method_descr.class == named_color_getter.class
                        && method_descr.method == named_color_getter.method
                        && method_descr.signature == named_color_getter.signature
                    {
                        invocation_positions.push(*pos);
                    }
                }
            }
        }

        // For each invocation, find the direct preceding Ldc and any jump branches
        for invocation_pos in invocation_positions {
            let mut results_for_invocation = Vec::new();

            // Find direct preceding Ldc
            let mut direct_ldc = None;
            for (pos, instr) in bytecode.0.iter().rev() {
                if *pos >= invocation_pos {
                    continue;
                }
                if let Instr::Ldc(id) = instr {
                    if let Some(color_name) = find_utf_ldc(&rp, *id as u16) {
                        direct_ldc = Some((*pos, color_name));
                        break;
                    }
                }
            }

            // Find immediate preceding jump that targets this invocation
            let mut jump_to_invocation = None;
            for (pos, instr) in bytecode.0.iter().rev() {
                if *pos >= invocation_pos {
                    continue;
                }
                match instr {
                    Instr::Goto(target) | Instr::GotoW(target) => {
                        if *target == invocation_pos {
                            jump_to_invocation = Some(*pos);
                            break;
                        }
                    }
                    _ => {}
                }
            }

            // If we have a direct Ldc, add it
            if let Some((ldc_pos, color_name)) = direct_ldc {
                results_for_invocation.push((ldc_pos, color_name));
            }

            // If we have a jump, find the first Ldc before that jump
            if let Some(jump_pos) = jump_to_invocation {
                for (pos, instr) in bytecode.0.iter().rev() {
                    if *pos >= jump_pos {
                        continue;
                    }
                    if let Instr::Ldc(id) = instr {
                        if let Some(color_name) = find_utf_ldc(&rp, *id as u16) {
                            results_for_invocation.push((*pos, color_name));
                            break;
                        }
                    }
                }
            }

            // Add all found strings for this invocation
            for (ldc_pos, color_name) in results_for_invocation {
                results.push((
                    color_name,
                    NamedColorGetterInvocation {
                        class: class_name.clone(),
                        method: method_name.clone(),
                        ldc_pos,
                    },
                ));
            }
        }
    }

    results
}
