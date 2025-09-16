use eframe::egui::{self, ProgressBar, Spinner};

#[derive(Default)]
pub struct StatusBar {
    pub progress: Option<Progress>,
}

pub struct Progress {
    group: &'static str,
    name: &'static str,
    value: Option<f32>,
}

impl Progress {
    pub fn new(group: &'static str, name: &'static str, value: Option<f32>) -> Self {
        Progress { group, name, value }
    }
}

pub fn status_bar(ui: &mut egui::Ui, status_bar: &StatusBar) {
    ui.horizontal(|ui| {
        const VERSION: &str = env!("CARGO_PKG_VERSION");
        ui.weak(format!("Cucumber v{}", VERSION));
        ui.separator();
        if let Some(progress) = &status_bar.progress {
            if let Some(value) = progress.value {
                ui.strong(progress.name);
                let prev = ui.visuals_mut().extreme_bg_color;
                ui.visuals_mut().extreme_bg_color = ui.visuals().faint_bg_color;
                ui.add(
                    ProgressBar::new(value)
                        .desired_height(4.0)
                        .desired_width(100.0),
                );
                ui.visuals_mut().extreme_bg_color = prev;
            } else {
                ui.strong(progress.name);
                ui.add(Spinner::new());
            }
        }
    });
}
