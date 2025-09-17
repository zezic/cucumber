use super::{
    extract_palette_color_methods, extract_raw_color_goodies, extract_release_metadata,
    scan_for_named_color_defs,
};
use crate::{
    jar::{
        debug::debug_print_color,
        goodies::{GeneralGoodies, MethodDescription, MethodSignatureKind},
        legacy::TimelineColorReference,
    },
    types::{Stage, StageProgress, ThemeProcessingEvent},
};
use krakatau2::lib::{
    classfile::{
        self,
        attrs::{AttrBody, Attribute},
        code::Instr,
        cpool::ConstPool,
        parse::Class,
    },
    disassemble::refprinter::{ConstData, FmimTag, PrimTag, RefPrinter, SingleTag},
    ParserOptions,
};
use krakatau2::zip::ZipArchive;
use std::collections::HashMap;
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
    let mut raw_color_goodies = None;
    let mut timeline_color_ref = None;
    let mut release_metadata = None;

    let mut data = Vec::new();

    report_progress(ThemeProcessingEvent {
        stage: Stage::ScanningClasses,
        progress: StageProgress::Percentage(0.0),
    });

    let mut init_class_name = None;
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
    })
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
