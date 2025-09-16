use eframe::egui;
use re_ui::UiExt;

pub fn right_panel(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.y = 0.0;

    ui.panel_content(|ui| {
        ui.section_collapsing_header("JAR").show(ui, |ui| {
            ui.label("hehehe");
        });
    });
}
