use eframe::egui;

pub fn central_panel(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.weak("Preview will be here in future version");
    });
}
