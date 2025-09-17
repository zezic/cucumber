use std::collections::HashMap;

use tracing::debug;

use crate::jar::goodies::ColorComponents;

pub fn debug_print_color(
    class_name: &str,
    color_name: &str,
    components: &ColorComponents,
    known_colors: &HashMap<String, ColorComponents>,
) {
    let Some((r, g, b)) = components.to_rgb(&known_colors) else {
        debug!("HSV Color: {:?}", components);
        return;
    };
    use colored::Colorize;
    let a = components.alpha().unwrap_or(255);

    let comp_line = format!("{} {} {} {}", r, g, b, a);

    let debug_line = if (r as u16 + g as u16 + b as u16) > 384 {
        format!("{} {}", comp_line, color_name)
            .black()
            .on_truecolor(r, g, b)
    } else {
        format!("{} {}", comp_line, color_name).on_truecolor(r, g, b)
    };
    debug!("{} ({})", debug_line, class_name);
}
