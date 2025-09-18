use std::collections::HashMap;
use std::io::Read;

use anyhow::anyhow;
use krakatau2::lib::{classfile, parse_utf8, ParserOptions};
use krakatau2::zip::ZipArchive;
use tracing::debug;

use crate::jar::{
    analysis::{
        introspection::{
            extract_named_color_getter_1, find_const_name, find_method_description,
            find_named_color_getter_1_invocations, find_utf_ldc,
        },
        scanner::{extract_release_metadata, is_useful_file, UsefulFileType},
    },
    core::assembly::init_refprinter,
    types::{
        colors::{ColorComponents, NamedColor, RawColorConst, RawColorConstants},
        metadata::{GeneralGoodies, RawColorGoodies},
        methods::{MethodDescription, MethodSignatureKind, PaletteColorMethods, RawColorMethods},
    },
    utils::legacy::TimelineColorReference,
};
use crate::types::{Stage, StageProgress, ThemeProcessingEvent};

use krakatau2::lib::{
    classfile::{attrs::AttrBody, code::Instr, parse::Class},
    disassemble::refprinter::{ConstData, RefPrinter},
};

/// Extract all general goodies from a JAR archive
pub fn extract_general_goodies<R: std::io::Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    mut report_progress: impl FnMut(ThemeProcessingEvent),
) -> anyhow::Result<GeneralGoodies> {
    use tracing::{debug, error, info};

    const PARSER_OPTIONS: ParserOptions = ParserOptions {
        no_short_code_attr: true,
    };

    info!("Starting extract_general_goodies");
    report_progress(Stage::LoadingFileNames.into());

    info!("Loading file names...");
    let file_names = zip.file_names().map(Into::into).collect::<Vec<String>>();
    info!("Loaded {} file names", file_names.len());

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
            info!("Found useful file: {} -> {:?}", file_name, useful_file_type);
            match useful_file_type {
                UsefulFileType::MainPalette => {
                    info!("Processing main palette: {}", file_name);
                    if let Some(methods) = extract_palette_color_methods(&class) {
                        info!("Successfully extracted palette color methods");
                        palette_color_meths = Some(methods);
                    } else {
                        error!("Failed to extract palette color methods from {}", file_name);
                    }
                }
                UsefulFileType::Init => {
                    info!("Processing init: {}", file_name);
                    init_class_name = Some(file_name.clone());
                }
                UsefulFileType::RawColor => {
                    info!("Processing raw color: {}", file_name);
                    if let Some(goodies) = extract_raw_color_goodies(&class) {
                        info!("Successfully extracted raw color goodies");
                        raw_color_goodies = Some(goodies);
                    } else {
                        error!("Failed to extract raw color goodies from {}", file_name);
                    }
                }
                UsefulFileType::CrashReport => {
                    info!("Processing crash report: {}", file_name);
                    if let Some(metadata) = extract_release_metadata(&class) {
                        info!("Successfully extracted release metadata");
                        release_metadata = Some(metadata);
                    } else {
                        error!("Failed to extract release metadata from {}", file_name);
                    }
                }
                UsefulFileType::NamedColorGetter1 => {
                    info!("Processing named color getter 1: {}", file_name);
                    deferred_named_color_getter_1_extraction = Some(file_name.clone());
                }
                UsefulFileType::TimelineColorCnst {
                    field_type_cp_idx,
                    fmim_idx,
                    cnst_name,
                } => {
                    info!("Processing timeline color const: {}", file_name);
                    timeline_color_ref = Some(TimelineColorReference {
                        class_filename: file_name.clone(),
                        const_name: cnst_name,
                        field_type_cp_idx,
                        fmim_idx,
                    });
                }
            }
        }

        let progress = (idx + 1) as f32 / file_names.len() as f32;
        if idx % 300 == 0 {
            report_progress(ThemeProcessingEvent {
                stage: Stage::ScanningClasses,
                progress: StageProgress::Percentage(progress),
            });
        }
    }

    info!("First pass complete, validating required components...");
    let palette_color_meths = palette_color_meths.ok_or_else(|| {
        error!("Palette not found in JAR");
        anyhow!("Palette not found")
    })?;
    let init_class_name = init_class_name.ok_or_else(|| {
        error!("Init class not found in JAR");
        anyhow!("Init class not found")
    })?;
    let raw_color_goodies = raw_color_goodies.ok_or_else(|| {
        error!("Raw color not found in JAR");
        anyhow!("Raw color not found")
    })?;
    info!("All required components found");

    // Extract named color getter if we found one
    if let Some(file_name) = deferred_named_color_getter_1_extraction {
        let mut file = zip.by_name(&file_name).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let class = classfile::parse(&data, PARSER_OPTIONS)
            .map_err(|e| anyhow::anyhow!("Parse error: {:?}", e))?;
        let raw_color_class = &raw_color_goodies.methods.rgba_f.class;
        if let Some(getter) = extract_named_color_getter_1(&class, raw_color_class) {
            named_color_getter_1 = Some(getter);
        }
    }

    let named_color_getter_1 =
        named_color_getter_1.ok_or_else(|| anyhow!("Named color getter not found"))?;
    info!("Named color getter found and extracted");

    info!("Starting second pass: extracting named colors and getter invocations");
    report_progress(ThemeProcessingEvent {
        stage: Stage::SearchingColorDefinitions,
        progress: StageProgress::Percentage(0.0),
    });

    // Second pass: extract named colors and getter invocations
    let mut named_colors = Vec::new();
    let mut known_colors = HashMap::new();

    for (idx, file_name) in file_names.iter().enumerate() {
        let mut file = zip.by_name(file_name).unwrap();
        data.clear();
        file.read_to_end(&mut data)?;
        let Ok(class) = classfile::parse(&data, PARSER_OPTIONS) else {
            continue;
        };

        let colors_in_file =
            scan_for_named_color_defs(&class, &palette_color_meths, file_name, &mut known_colors);
        if !colors_in_file.is_empty() {
            debug!("Found {} colors in {}", colors_in_file.len(), file_name);
        }
        named_colors.extend(colors_in_file);

        let getter_invocations =
            find_named_color_getter_1_invocations(&class, &named_color_getter_1);
        if !getter_invocations.is_empty() {
            debug!(
                "Found {} getter invocations in {}",
                getter_invocations.len(),
                file_name
            );
        }
        for (key, invocation) in getter_invocations {
            named_color_getter_invocations.push((key, invocation));
        }

        let progress = (idx + 1) as f32 / file_names.len() as f32;
        if idx % 300 == 0 {
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

    info!("Successfully extracted general goodies:");
    info!("  - {} named colors", named_colors.len());
    info!(
        "  - {} getter invocations",
        named_color_getter_invocations.len()
    );
    info!(
        "  - {} release metadata entries",
        release_metadata.as_ref().map_or(0, |m| m.len())
    );

    Ok(GeneralGoodies {
        init_class: init_class_name,
        named_colors,
        palette_color_methods: palette_color_meths,
        raw_colors: raw_color_goodies,
        timeline_color_ref,
        release_metadata: release_metadata.unwrap_or_default(),
        named_color_getter_1,
        named_color_getter_invocations,
    })
}

/// Scan a class for named color definitions
pub fn scan_for_named_color_defs(
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
                            match sig_kind.extract_color_components(idx, bytecode, &rp) {
                                Ok(components) => {
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
                                Err(e) => {
                                    debug!("Failed to extract color components: {}", e);
                                    debug!(
                                        "  Context: class={}, method_idx={}, bytecode_idx={}",
                                        class_name, method_idx, idx
                                    );
                                    debug!("  Signature kind: {:?}", sig_kind);
                                    // Continue processing instead of crashing
                                }
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

/// Extract palette color methods from a class
pub fn extract_palette_color_methods(class: &Class) -> Option<PaletteColorMethods> {
    let rp = init_refprinter(&class.cp, &class.attrs);

    let main_palette_method = class.methods.iter().skip(1).next()?;
    let attr = main_palette_method.attrs.first()?;
    let AttrBody::Code((code_1, _)) = &attr.body else {
        return None;
    };

    let bytecode = &code_1.bytecode;

    let invokes = bytecode
        .0
        .iter()
        .filter_map(|(_pos, ix)| match ix {
            Instr::Invokevirtual(method_id) => Some(method_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    // Updated signatures to match evolved Bitwig JAR format
    // Methods now take a String name as first parameter and return color objects
    let grayscale_i = find_method("(Ljava/lang/String;I)", None, None, &invokes, &rp)?;
    let rgb_i = find_method("(Ljava/lang/String;III)", None, None, &invokes, &rp)?;
    let rgba_i_absolute = find_method("(Ljava/lang/String;IIII)", None, None, &invokes, &rp)?;
    let rgba_i_blended_on_background =
        find_method("(Ljava/lang/String;IIII)", None, Some(1), &invokes, &rp)?;
    let hsv_f_relative_to_background =
        find_method("(Ljava/lang/String;FFF)", None, None, &invokes, &rp)?;

    // Try to find reference-based HSV method (with color object reference)
    let ref_hsv_f = find_method("(Ljava/lang/String;L", None, None, &invokes, &rp)
        .filter(|desc| desc.signature.contains("FFF)"));

    // String-based HSV method should be the same as the standard one now
    let name_hsv_f = find_method("(Ljava/lang/String;FFF)", None, None, &invokes, &rp);

    Some(PaletteColorMethods {
        grayscale_i,
        rgb_i,
        rgba_i_absolute,
        rgba_i_blended_on_background,
        hsv_f_relative_to_background: hsv_f_relative_to_background.clone(),
        ref_hsv_f: ref_hsv_f.unwrap_or_else(|| hsv_f_relative_to_background.clone()),
        name_hsv_f: name_hsv_f.unwrap_or_else(|| hsv_f_relative_to_background.clone()),
    })
}

/// Extract raw color goodies from a class
pub fn extract_raw_color_goodies(class: &Class) -> Option<RawColorGoodies> {
    let rp = init_refprinter(&class.cp, &class.attrs);

    let class_name = class.cp.clsutf(class.this).and_then(parse_utf8)?;

    let mut rgbaf_desc = None;
    let mut rgbad_desc = None;

    // At first, find all popular constructors
    for method in &class.methods {
        let Some(meth_name) = class.cp.utf8(method.name).and_then(parse_utf8) else {
            continue;
        };
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
        rgba_f: rgbaf_desc?,
        rgba_d: rgbad_desc?,
    };

    let mut consts = Vec::new();

    // Now, find important constants in class initializer
    for method in &class.methods {
        let Some(meth_name) = class.cp.utf8(method.name).and_then(parse_utf8) else {
            continue;
        };
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
                        let comps = match raw_color_meth
                            .signature_kind
                            .as_ref()
                            .unwrap()
                            .extract_color_components(idx, bytecode, &rp)
                        {
                            Ok(components) => components,
                            Err(e) => {
                                debug!("Failed to extract raw color components: {}", e);
                                debug!(
                                    "  Context: method={:?}, bytecode_idx={}",
                                    raw_color_meth, idx
                                );
                                continue; // Skip this color and continue with next
                            }
                        };
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
            }
        }
    }

    Some(RawColorGoodies {
        methods: raw_color_methods,
        constants: RawColorConstants { consts },
    })
}

/// Find a method with specific signature and optional parameters
fn find_method(
    signature_start: &str,
    color_rec_name: Option<&str>,
    skip: Option<usize>,
    invokes: &[&u16],
    rp: &RefPrinter<'_>,
) -> Option<MethodDescription> {
    let skip = skip.unwrap_or(0);
    for method_id in invokes.iter().skip(skip) {
        let desc = find_method_description(rp, **method_id, color_rec_name)?;
        if desc.signature.starts_with(signature_start) {
            return Some(desc);
        }
    }
    None
}
