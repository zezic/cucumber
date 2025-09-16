use eframe::egui;

use crate::ui::commands::{CommandSender, CucumberCommandSender};

use super::commands::CucumberCommand;

// NOTE: Relying on `self` is dangerous, as this is called during a time where some internal
// fields may have been temporarily `take()`n out. Keep this a static method.
pub fn handle_dropping_files(egui_ctx: &egui::Context, command_sender: &CommandSender) {
    #![allow(clippy::needless_continue)] // false positive, depending on target_arch

    preview_files_being_dropped(egui_ctx);

    let dropped_files = egui_ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));

    if dropped_files.is_empty() {
        return;
    }

    for file in dropped_files {
        let Some(path) = file.path else {
            continue;
        };
        let Some(text) = path.to_str() else {
            continue;
        };

        if text.to_lowercase().ends_with("jar") {
            command_sender.send_ui(CucumberCommand::LoadJar(Some(path)));
        }
    }
}

fn preview_files_being_dropped(egui_ctx: &egui::Context) {
    use egui::{Align2, Id, LayerId, Order, TextStyle};

    // Preview hovering files:
    if !egui_ctx.input(|i| i.raw.hovered_files.is_empty()) {
        use std::fmt::Write as _;

        let mut text = "Drop to load:\n".to_owned();
        egui_ctx.input(|input| {
            for file in &input.raw.hovered_files {
                if let Some(path) = &file.path {
                    write!(text, "\n{}", path.display()).ok();
                } else if !file.mime.is_empty() {
                    write!(text, "\n{}", file.mime).ok();
                }
            }
        });

        let painter =
            egui_ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

        let screen_rect = egui_ctx.screen_rect();
        painter.rect_filled(
            screen_rect,
            0.0,
            egui_ctx
                .style()
                .visuals
                .extreme_bg_color
                .gamma_multiply_u8(192),
        );
        painter.text(
            screen_rect.center(),
            Align2::CENTER_CENTER,
            text,
            TextStyle::Body.resolve(&egui_ctx.style()),
            egui_ctx.style().visuals.strong_text_color(),
        );
    }
}
