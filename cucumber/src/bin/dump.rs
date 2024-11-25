use std::{env, fs::{self, File}, io::BufWriter};

use cucumber::{extract_general_goodies, types::{AbsoluteColor, ColorConst, CucumberBitwigTheme, NamedColor, UiTarget}};
use krakatau2::zip;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let input_jar = &args[1];
    let output_json = &args[2];

    let file = fs::File::open(input_jar)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let general_goodies = extract_general_goodies(&mut zip, |_| {})?;

    let mut theme = CucumberBitwigTheme {
        name: "Extracted Theme".into(),
        ..Default::default()
    };

    let known_colors = general_goodies.named_colors.iter().map(|color| {
        (color.color_name.clone(), color.components.clone())
    }).collect();

    for color in general_goodies.named_colors {
        let (h, s, v) = color.components.to_hsv(&known_colors);
        let a = color.components.alpha().unwrap_or(255);
        let named_color = NamedColor::Absolute(
            AbsoluteColor {
                h, s, v, a: a as f32 / 255.0, compositing_mode: color.compositing_mode
            }
        );
        theme.named_colors.insert(color.color_name.clone(), named_color);
    }

    if let Some(timeline_color_ref) = general_goodies.timeline_color_ref {
        let timeline_const_name = timeline_color_ref.const_name;
        let timeline_const = general_goodies.raw_colors.constants.consts.iter().find(|cnst| {
            cnst.const_name == timeline_const_name
        }).unwrap();
        let (r, g, b) = timeline_const.color_comps.to_rgb(&known_colors);
        let a = timeline_const.color_comps.alpha().unwrap_or(255);

        let timeline_color_const = ColorConst::from_comps(r, g, b, a);

        theme.constant_refs.insert(UiTarget::Playhead, timeline_color_const);
    }

    let file = File::create(output_json).expect("Unable to create file");
    let writer = BufWriter::new(file);

    serde_json::to_writer_pretty(writer, &theme).unwrap();

    Ok(())
}