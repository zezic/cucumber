use std::collections::BTreeMap;

use eframe::{
    egui::{
        self,
        color_picker::{color_picker_hsva_2d, Alpha},
        Rgba, TextStyle,
    },
    epaint::Hsva,
};
use re_ui::{
    list_item::{list_item_scope, ListItemContentButtonsExt, PropertyContent},
    UiExt,
};
use tracing::info;

use crate::types::{
    AbsoluteColor, CucumberBitwigTheme,
    NamedColor::{self, Absolute},
};

pub fn left_panel(
    ui: &mut egui::Ui,
    theme: Option<&mut CucumberBitwigTheme>,
    selected_color: &mut Option<String>,
    changed_colors: &mut BTreeMap<String, NamedColor>,
) {
    let orig_spacing = ui.spacing_mut().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;

    ui.panel_content(|ui| {
        ui.panel_title_bar("Palette", None);
    });

    let available_width = ui.available_width();

    let title_height = ui
        .style()
        .text_styles
        .get(&TextStyle::Body)
        .map(|font_id| font_id.size)
        .unwrap_or(16.0);

    let color_picker_height = ui.tokens().panel_margin().sum().y // title margin
        + title_height // font size
        // + orig_spacing // padding under title
        + ui.spacing().interact_size.y // inputs
        + ui.spacing().interact_size.y // preview
        + available_width // color square
        + ui.spacing().interact_size.y // slider 1
        + ui.spacing().interact_size.y // slider 2
        + orig_spacing * 4.0 // some paddings
        + orig_spacing; // padding under sliders

    let scroll_height = if theme.is_some() && selected_color.is_some() {
        ui.available_height() - color_picker_height
    } else {
        ui.available_height()
    };

    if let Some(theme) = &theme {
        egui::ScrollArea::vertical()
            .id_salt("palette_scroll")
            .auto_shrink([false, false])
            .max_height(scroll_height)
            .show(ui, |ui| {
                ui.panel_content(|ui| {
                    list_item_scope(ui, "layout tree", |ui| {
                        colors_list(ui, theme, selected_color, changed_colors);
                    });
                });
            });
    }

    if let (Some(theme), Some(selected_color_name)) = (theme, &selected_color) {
        let maybe_color = theme.named_colors.get_mut(selected_color_name);
        if let Some(NamedColor::Absolute(absolute_color)) = maybe_color {
            let deselect = color_picker(
                ui,
                orig_spacing,
                absolute_color,
                selected_color_name,
                changed_colors,
            );
            if deselect {
                *selected_color = None;
            }
        };
    }
}

fn colors_list(
    ui: &mut egui::Ui,
    theme: &CucumberBitwigTheme,
    selected_color: &mut Option<String>,
    changed_colors: &mut BTreeMap<String, NamedColor>,
) {
    let desired_width = ui.available_width();
    for (color_name, named_color) in &theme.named_colors {
        let content = PropertyContent::new(color_name)
            .min_desired_width(desired_width)
            .with_icon(&re_ui::icons::ENTITY)
            .with_action_button(&re_ui::icons::SEARCH, "Search usage", || {
                info!("Kkekek");
            });
        let mut color: [u8; 4];
        let content = match named_color {
            Absolute(absolute_color) => {
                color = [
                    absolute_color.r,
                    absolute_color.g,
                    absolute_color.b,
                    absolute_color.a,
                ];
                content.value_color_mut(&mut color)
            }
            NamedColor::Relative(_) => content,
        };

        let selected = matches!(selected_color, Some(selected) if selected == color_name);
        let list_item_response = ui.list_item().selected(selected).show_flat(ui, content);

        if list_item_response.clicked() {
            *selected_color = Some(color_name.clone());
        }

        if list_item_response.changed() {
            changed_colors.insert(color_name.clone(), named_color.clone());
        }
    }
}

fn color_picker(
    ui: &mut egui::Ui,
    orig_spacing: f32,
    absolute_color: &mut AbsoluteColor,
    selected_color_name: &String,
    changed_colors: &mut BTreeMap<String, NamedColor>,
) -> bool {
    let mut deselect = false;

    let max_title_chars = (ui.available_width() * 0.12) as usize;
    let label = if selected_color_name.len() > max_title_chars {
        &format!("{}...", &selected_color_name[..max_title_chars])
    } else {
        selected_color_name
    };

    ui.panel_content(|ui| {
        ui.panel_title_bar_with_buttons(label, Some(selected_color_name.as_str()), |ui| {
            if ui
                .small_icon_button(&re_ui::icons::CLOSE, "Deselect color")
                .clicked()
            {
                deselect = true;
            }
        });

        ui.spacing_mut().item_spacing.y = orig_spacing;
        ui.add_space(10.0);
        let rgba = Rgba::from_srgba_unmultiplied(
            absolute_color.r,
            absolute_color.g,
            absolute_color.b,
            absolute_color.a,
        );
        let mut hsva = Hsva::from(rgba);

        ui.spacing_mut().slider_width = ui.available_width();

        if color_picker_hsva_2d(ui, &mut hsva, Alpha::OnlyBlend) {
            let [r, g, b, a] = hsva.to_srgba_unmultiplied();
            absolute_color.r = r;
            absolute_color.g = g;
            absolute_color.b = b;
            absolute_color.a = a;

            changed_colors.insert(
                selected_color_name.clone(),
                NamedColor::Absolute(absolute_color.clone()),
            );
        }
    });

    deselect
}
