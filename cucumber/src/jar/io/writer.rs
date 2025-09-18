use std::{
    collections::{BTreeMap, HashMap},
    io::Read,
    path::Path,
};

use anyhow::anyhow;

use krakatau2::{
    file_output_util::Writer,
    lib::{classfile, ParserOptions},
    zip,
};
use tracing::warn;

use crate::{
    jar::{
        analysis::extractor::extract_general_goodies,
        core::assembly::reasm,
        modification::{color_replacer::replace_named_color, patcher::patch_class},
        types::colors::ColorComponents,
    },
    types::{CompositingMode, NamedColor, Stage, StageProgress, ThemeProcessingEvent},
};

pub fn write_theme_to_jar(
    jar_in: impl AsRef<Path>,
    jar_out: impl AsRef<Path>,
    changed_colors: BTreeMap<String, NamedColor>,
    mut report_progress: impl FnMut(ThemeProcessingEvent),
) -> anyhow::Result<()> {
    report_progress(ThemeProcessingEvent {
        stage: Stage::WritingTheme,
        progress: StageProgress::Unknown,
    });

    let file = std::fs::File::open(jar_in)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let mut general_goodies = extract_general_goodies(&mut zip, |_| {})?;
    let mut patched_classes = HashMap::new();

    let mut file = zip.by_name(&general_goodies.init_class)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    let mut class = classfile::parse(
        &buffer,
        ParserOptions {
            no_short_code_attr: true,
        },
    )
    .map_err(|err| anyhow!("Parse: {:?}", err))?;
    patch_class(&mut class);
    let patched = reasm(&class).unwrap();
    patched_classes.insert(file.name().to_string(), patched);
    drop(file);

    let named_colors_copy = general_goodies.named_colors.clone();
    for jar_color in named_colors_copy {
        let Some(NamedColor::Absolute(absolute_color)) = changed_colors.get(&jar_color.color_name)
        else {
            continue;
        };

        if !matches!(absolute_color.compositing_mode, CompositingMode::Absolute) {
            // TODO: Detach compositing mode from absolute colors, it meant to be used with relative colors
            warn!(
                "Compositing mode is not implemented for absolute colors: {}",
                jar_color.color_name
            );
            continue;
        }

        let file_name_w_ext = format!("{}.class", jar_color.class_name);
        let buffer = match patched_classes.remove(&file_name_w_ext) {
            Some(patched) => patched,
            None => {
                let mut file = zip.by_name(&file_name_w_ext)?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                buffer
            }
        };

        let mut class = classfile::parse(
            &buffer,
            ParserOptions {
                no_short_code_attr: true,
            },
        )
        .map_err(|err| anyhow!("Parse: {:?}", err))?;

        let new_value = ColorComponents::Rgbai(
            absolute_color.r,
            absolute_color.g,
            absolute_color.b,
            absolute_color.a,
        );

        if replace_named_color(
            &mut class,
            &jar_color.color_name,
            new_value,
            &mut general_goodies.named_colors,
            &general_goodies.palette_color_methods,
            absolute_color.compositing_mode.clone(),
        )
        .is_none()
        {
            warn!("failed to replace in {}", file_name_w_ext);
        }

        let new_buffer = reasm(&class)?;
        patched_classes.insert(file_name_w_ext, new_buffer);
    }

    let mut writer = Writer::new(jar_out.as_ref())?;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_owned();

        let buffer = match patched_classes.remove(&name) {
            Some(patched) => patched,
            None => {
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer)?;
                buffer
            }
        };

        writer.write(Some(&name), &buffer)?;
    }
    report_progress(ThemeProcessingEvent {
        stage: Stage::WritingTheme,
        progress: StageProgress::Done,
    });

    Ok(())
}
