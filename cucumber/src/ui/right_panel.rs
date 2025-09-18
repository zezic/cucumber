use eframe::egui;
use re_ui::{list_item::PropertyContent, UiExt};

use crate::jar::goodies::GeneralGoodies;

pub fn right_panel(ui: &mut egui::Ui, goodies: &Option<GeneralGoodies>) {
    let orig_spacing = ui.spacing_mut().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;

    ui.panel_content(|ui| {
        ui.section_collapsing_header("JAR").show(ui, |ui| {
            if let Some(goodies) = goodies {
                ui.list_item_scope("jar_info", |ui| {
                    for (key, value) in &goodies.release_metadata {
                        ui.list_item()
                            .show_flat(ui, PropertyContent::new(key).value_text(value));
                    }

                    ui.add_space(orig_spacing);
                    ui.full_span_separator();
                    ui.add_space(orig_spacing);

                    ui.label(&format!(
                        "Named Color Getter 1: {:#?}",
                        goodies.named_color_getter_1
                    ));
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.weak("Drop some JAR here");
                });
            }
        });

        if let Some(goodies) = goodies {
            ui.section_collapsing_header("Getter Invocations")
                .show(ui, |ui| {
                    ui.list_item_scope("getter_invocations", |ui| {
                        for (color_name, invocation) in &goodies.named_color_getter_invocations {
                            ui.list_item().show_flat(
                                ui,
                                PropertyContent::new(color_name).value_text(&invocation.class),
                            );
                        }
                    });
                });
        }
    });
}
