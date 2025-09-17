use super::{
    extract_palette_color_methods, extract_raw_color_goodies, extract_release_metadata,
    is_useful_file, scan_for_named_color_defs, UsefulFileType,
};
use crate::{
    jar::{debug::debug_print_color, goodies::GeneralGoodies, legacy::TimelineColorReference},
    types::{Stage, StageProgress, ThemeProcessingEvent},
};
use krakatau2::lib::{classfile, ParserOptions};
use krakatau2::zip::ZipArchive;
use std::collections::HashMap;
use std::io::Read;
use tracing::debug;

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
