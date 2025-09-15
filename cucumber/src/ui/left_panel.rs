use eframe::egui;
use re_ui::{
    list_item::{list_item_scope, LabelContent, ListItemContentButtonsExt, PropertyContent},
    UiExt,
};
use tracing::info;

use crate::types::CucumberBitwigTheme;

pub fn left_panel(
    ui: &mut egui::Ui,
    theme: &Option<CucumberBitwigTheme>,
    selected_color: &mut Option<String>,
) {
    ui.spacing_mut().item_spacing.y = 0.0;

    ui.panel_content(|ui| {
        ui.panel_title_bar("Root", None);
    });

    if let Some(theme) = theme {
        egui::ScrollArea::vertical()
            .id_salt("layout_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.panel_content(|ui| {
                    list_item_scope(ui, "layout tree", |ui| {
                        colors_list(ui, theme, selected_color);
                    });
                });
            });
    }
}

fn colors_list(
    ui: &mut egui::Ui,
    theme: &CucumberBitwigTheme,
    selected_color: &mut Option<String>,
) {
    let desired_width = ui.available_width();
    for (color_name, color) in &theme.named_colors {
        let content = PropertyContent::new(color_name)
            .min_desired_width(desired_width)
            .with_icon(&re_ui::icons::ENTITY)
            .with_action_button(&re_ui::icons::EDIT, "Edit", || {
                info!("Kkekek");
            });

        let selected = matches!(selected_color, Some(selected) if selected == color_name);
        if ui
            .list_item()
            .selected(selected)
            .show_flat(ui, content)
            .clicked()
        {
            *selected_color = Some(color_name.clone());
        }
    }
}
