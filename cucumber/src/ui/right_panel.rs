use eframe::egui;
use re_ui::{list_item::PropertyContent, UiExt};

use crate::GeneralGoodies;

pub fn right_panel(ui: &mut egui::Ui, goodies: &Option<GeneralGoodies>) {
    ui.spacing_mut().item_spacing.y = 0.0;

    ui.panel_content(|ui| {
        ui.section_collapsing_header("JAR").show(ui, |ui| {
            if let Some(goodies) = goodies {
                ui.list_item_scope("jar_info", |ui| {
                    for (key, value) in &goodies.release_metadata {
                        ui.list_item()
                            .show_flat(ui, PropertyContent::new(key).value_text(value));
                    }
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.weak("Drop some JAR here");
                });
            }
        });
    });
}
