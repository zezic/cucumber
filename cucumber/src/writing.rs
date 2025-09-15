use std::{collections::HashMap, io::Read, path::Path};

use anyhow::anyhow;

use eframe::epaint::Hsva;
use krakatau2::{
    file_output_util::Writer,
    lib::{classfile, ParserOptions},
    zip,
};
use tracing::{debug, warn};

use crate::{
    extract_general_goodies,
    patching::patch_class,
    reasm, replace_named_color,
    types::{CompositingMode, CucumberBitwigTheme, NamedColor},
    ui::ThemeWritingEvent,
    ColorComponents,
};

pub fn write_theme_to_jar(
    jar_in: String,
    jar_out: String,
    theme: CucumberBitwigTheme,
    mut report_progress: impl FnMut(ThemeWritingEvent),
) -> anyhow::Result<()> {
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
    let patched = reasm(file.name(), &class).unwrap();
    patched_classes.insert(file.name().to_string(), patched);
    drop(file);

    let named_colors_copy = general_goodies.named_colors.clone();
    for clr in named_colors_copy {
        let file_name_w_ext = format!("{}.class", clr.class_name);
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

        if let Some(NamedColor::Absolute(repl)) = theme.named_colors.get(&clr.color_name) {
            let [r, g, b, a] = Hsva::new(repl.h, repl.s, repl.v, repl.a).to_srgba_unmultiplied();

            let new_value = match repl.compositing_mode {
                Some(CompositingMode::RelativeToBackground) => {
                    ColorComponents::Hsvf(repl.h, repl.s, repl.v)
                }
                _ => ColorComponents::Rgbai(r, g, b, a),
            };
            if replace_named_color(
                &mut class,
                &clr.color_name,
                new_value,
                &mut general_goodies.named_colors,
                &general_goodies.palette_color_methods,
                repl.compositing_mode.clone(),
            )
            .is_none()
            {
                warn!("failed to replace in {}", file_name_w_ext);
            }

            let new_buffer = reasm(&file_name_w_ext, &class)?;
            patched_classes.insert(file_name_w_ext, new_buffer);
        }
    }

    let mut writer = Writer::new(Path::new(&jar_out))?;

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
    report_progress(ThemeWritingEvent::Done);

    Ok(())
}
