use eframe::egui;

pub fn central_panel(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.label("Preview will be there in future version");
    });
}
