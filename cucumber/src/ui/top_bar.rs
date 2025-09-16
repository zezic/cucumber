#[cfg(not(target_arch = "wasm32"))]
use eframe::egui::{self, os::OperatingSystem};
use re_ui::{ContextExt, UiExt};

use crate::ui::{commands::CommandSender, commands::ScopeCommand, PanelsState};

pub fn top_bar(
    command_sender: &CommandSender,
    mini_state: &mut PanelsState,
    egui_ctx: &egui::Context,
) {
    let top_bar_style = egui_ctx.top_bar_style(false);

    egui::TopBottomPanel::top("top_bar").show(egui_ctx, |ui| {
        #[cfg(not(target_arch = "wasm32"))]
        if !re_ui::native_window_bar(eframe::egui::os::OperatingSystem::default()) {
            // Interact with background first, so that buttons in the top bar gets input priority
            // (last added widget has priority for input).
            let title_bar_response = ui.interact(
                ui.max_rect(),
                ui.id().with("background"),
                egui::Sense::click(),
            );
            if title_bar_response.double_clicked() {
                let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            } else if title_bar_response.is_pointer_button_down_on() {
                // TODO(emilk): This should probably only run on `title_bar_response.drag_started_by(PointerButton::Primary)`,
                // see https://github.com/emilk/egui/pull/4656
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
        }

        egui::MenuBar::new().ui(ui, |ui| {
            ui.set_height(top_bar_style.height);
            ui.add_space(top_bar_style.indent);

            ui.menu_button("File", |ui| file_menu(ui, command_sender));
            ui.menu_button("View", |ui| view_menu(ui, command_sender));
            // if ui
            //     .button("Dump tree")
            //     .on_hover_ui(|ui| ScopeCommand::DumpTree.tooltip_ui(ui))
            //     .clicked()
            // {
            //     command_sender.send_ui(ScopeCommand::DumpTree);
            // }

            top_bar_ui(mini_state, ui);
        });
    });
}

fn top_bar_ui(mini_state: &mut PanelsState, ui: &mut egui::Ui) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // From right-to-left:

        if re_ui::CUSTOM_WINDOW_DECORATIONS {
            ui.add_space(8.0);
            ui.native_window_buttons_ui();
            ui.separator();
        } else {
            ui.add_space(16.0);
        }

        ui.medium_icon_toggle_button(
            &re_ui::icons::RIGHT_PANEL_TOGGLE,
            "Show right panel",
            &mut mini_state.show_right_panel,
        );
        ui.medium_icon_toggle_button(
            &re_ui::icons::BOTTOM_PANEL_TOGGLE,
            "Show bottom panel",
            &mut mini_state.show_bottom_panel,
        );
        ui.medium_icon_toggle_button(
            &re_ui::icons::LEFT_PANEL_TOGGLE,
            "Show left panel",
            &mut mini_state.show_left_panel,
        );
    });
}

fn file_menu(ui: &mut egui::Ui, command_sender: &CommandSender) {
    ScopeCommand::ToggleSourceCreator.menu_button_ui(ui, command_sender);
    ui.separator();
    ScopeCommand::AddOscillograph.menu_button_ui(ui, command_sender);
    ScopeCommand::AddValues.menu_button_ui(ui, command_sender);
    ScopeCommand::AddXyPlot.menu_button_ui(ui, command_sender);
    ui.separator();
    ScopeCommand::WriteValuesCSV.menu_button_ui(ui, command_sender);
    ui.separator();
    ScopeCommand::Quit.menu_button_ui(ui, command_sender);
}

fn view_menu(ui: &mut egui::Ui, command_sender: &CommandSender) {
    ScopeCommand::ToggleFullscreen.menu_button_ui(ui, command_sender);
    ScopeCommand::ToggleTheme.menu_button_ui(ui, command_sender);
    ui.separator();
    ScopeCommand::ZoomIn.menu_button_ui(ui, command_sender);
    ScopeCommand::ZoomOut.menu_button_ui(ui, command_sender);
    ScopeCommand::ZoomReset.menu_button_ui(ui, command_sender);
    ui.separator();
    ScopeCommand::ToggleCommandPalette.menu_button_ui(ui, command_sender);
}
