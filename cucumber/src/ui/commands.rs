use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use re_ui::ContextExt;
use smallvec::{smallvec, SmallVec};

/// Interface for sending [`UICommand`] messages.
pub trait CucumberCommandSender {
    fn send_ui(&self, command: CucumberCommand);
}

/// All the commands we support.
///
/// Most are available in the GUI,
/// some have keyboard shortcuts,
/// and all are visible in the [`crate::CommandPalette`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, strum_macros::EnumIter)]
pub enum CucumberCommand {
    #[cfg(not(target_arch = "wasm32"))]
    Quit,
    // Settings,
    ToggleTheme,
    ToggleFullscreen,
    ToggleCommandPalette,

    #[cfg(not(target_arch = "wasm32"))]
    ZoomIn,
    #[cfg(not(target_arch = "wasm32"))]
    ZoomOut,
    #[cfg(not(target_arch = "wasm32"))]
    ZoomReset,

    SaveJar,
}

impl CucumberCommand {
    pub fn text(self) -> &'static str {
        self.text_and_tooltip().0
    }

    pub fn tooltip(self) -> &'static str {
        self.text_and_tooltip().1
    }

    pub fn text_and_tooltip(self) -> (&'static str, &'static str) {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Quit => ("Quit", "Close the Rerun Viewer"),

            // Self::Settings => ("Settings…", "Show the settings screen"),
            Self::ToggleTheme => ("Toggle theme", "Switch between light and dark themes"),

            #[cfg(not(target_arch = "wasm32"))]
            Self::ToggleFullscreen => (
                "Toggle fullscreen",
                "Toggle between windowed and fullscreen viewer",
            ),

            #[cfg(target_arch = "wasm32")]
            Self::ToggleFullscreen => (
                "Toggle fullscreen",
                "Toggle between full viewport dimensions and initial dimensions",
            ),

            #[cfg(not(target_arch = "wasm32"))]
            Self::ZoomIn => ("Zoom in", "Increases the UI zoom level"),
            #[cfg(not(target_arch = "wasm32"))]
            Self::ZoomOut => ("Zoom out", "Decreases the UI zoom level"),
            #[cfg(not(target_arch = "wasm32"))]
            Self::ZoomReset => (
                "Reset zoom",
                "Resets the UI zoom level to the operating system's default value",
            ),

            Self::ToggleCommandPalette => ("Command palette…", "Toggle the Command Palette"),

            Self::SaveJar => ("Save JAR", "Save the current workspace to a JAR file"),
        }
    }

    /// All keyboard shortcuts, with the primary first.
    pub fn kb_shortcuts(self) -> SmallVec<[KeyboardShortcut; 2]> {
        fn key(key: Key) -> KeyboardShortcut {
            KeyboardShortcut::new(Modifiers::NONE, key)
        }

        #[allow(dead_code)]
        fn ctrl(key: Key) -> KeyboardShortcut {
            KeyboardShortcut::new(Modifiers::CTRL, key)
        }

        fn cmd(key: Key) -> KeyboardShortcut {
            KeyboardShortcut::new(Modifiers::COMMAND, key)
        }

        #[allow(dead_code)]
        fn alt(key: Key) -> KeyboardShortcut {
            KeyboardShortcut::new(Modifiers::ALT, key)
        }

        #[allow(dead_code)]
        fn cmd_shift(key: Key) -> KeyboardShortcut {
            KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, key)
        }

        #[allow(dead_code)]
        fn cmd_alt(key: Key) -> KeyboardShortcut {
            KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::ALT, key)
        }

        #[allow(dead_code)]
        fn ctrl_shift(key: Key) -> KeyboardShortcut {
            KeyboardShortcut::new(Modifiers::CTRL | Modifiers::SHIFT, key)
        }

        match self {
            #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
            Self::Quit => smallvec![KeyboardShortcut::new(Modifiers::ALT, Key::F4)],

            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "windows")))]
            Self::Quit => smallvec![cmd(Key::Q)],

            // Self::Settings => smallvec![cmd(Key::Comma)],
            Self::ToggleTheme => smallvec![key(Key::F12)],

            #[cfg(not(target_arch = "wasm32"))]
            Self::ToggleFullscreen => smallvec![key(Key::F11)],
            #[cfg(target_arch = "wasm32")]
            Self::ToggleFullscreen => smallvec![],

            Self::ToggleCommandPalette => smallvec![cmd(Key::P)],

            #[cfg(not(target_arch = "wasm32"))]
            Self::ZoomIn => smallvec![egui::gui_zoom::kb_shortcuts::ZOOM_IN],
            #[cfg(not(target_arch = "wasm32"))]
            Self::ZoomOut => smallvec![egui::gui_zoom::kb_shortcuts::ZOOM_OUT],
            #[cfg(not(target_arch = "wasm32"))]
            Self::ZoomReset => smallvec![egui::gui_zoom::kb_shortcuts::ZOOM_RESET],

            Self::SaveJar => smallvec![cmd(Key::S)],
        }
    }

    /// Primary keyboard shortcut
    pub fn primary_kb_shortcut(self) -> Option<KeyboardShortcut> {
        self.kb_shortcuts().first().copied()
    }

    /// Return the keyboard shortcut for this command, nicely formatted
    // TODO(emilk): use Help/IconText instead
    pub fn formatted_kb_shortcut(self, egui_ctx: &egui::Context) -> Option<String> {
        // Note: we only show the primary shortcut to the user.
        // The fallbacks are there for people who have muscle memory for the other shortcuts.
        self.primary_kb_shortcut()
            .map(|shortcut| egui_ctx.format_shortcut(&shortcut))
    }

    pub fn icon(self) -> Option<&'static re_ui::Icon> {
        match self {
            _ => None,
        }
    }

    #[must_use = "Returns the Command that was triggered by some keyboard shortcut"]
    pub fn listen_for_kb_shortcut(egui_ctx: &egui::Context) -> Option<Self> {
        use strum::IntoEnumIterator as _;

        let anything_has_focus = egui_ctx.memory(|mem| mem.focused().is_some());
        if anything_has_focus {
            return None; // e.g. we're typing in a TextField
        }

        let mut commands: Vec<(KeyboardShortcut, Self)> = Self::iter()
            .flat_map(|cmd| {
                cmd.kb_shortcuts()
                    .into_iter()
                    .map(move |kb_shortcut| (kb_shortcut, cmd))
            })
            .collect();

        // If the user pressed `Cmd-Shift-S` then egui will match that
        // with both `Cmd-Shift-S` and `Cmd-S`.
        // The reason is that `Shift` (and `Alt`) are sometimes required to produce certain keys,
        // such as `+` (`Shift =` on an american keyboard).
        // The result of this is that we must check for `Cmd-Shift-S` before `Cmd-S`, etc.
        // So we order the commands here so that the commands with `Shift` and `Alt` in them
        // are checked first.
        commands.sort_by_key(|(kb_shortcut, _cmd)| {
            let num_shift_alts =
                kb_shortcut.modifiers.shift as i32 + kb_shortcut.modifiers.alt as i32;
            -num_shift_alts // most first
        });

        egui_ctx.input_mut(|input| {
            for (kb_shortcut, command) in commands {
                if input.consume_shortcut(&kb_shortcut) {
                    return Some(command);
                }
            }
            None
        })
    }

    /// Show this command as a menu-button.
    ///
    /// If clicked, enqueue the command.
    pub fn menu_button_ui(
        self,
        ui: &mut egui::Ui,
        command_sender: &impl CucumberCommandSender,
    ) -> egui::Response {
        let button = self.menu_button(ui.ctx());
        let response = ui.add(button).on_hover_text(self.tooltip());

        if response.clicked() {
            command_sender.send_ui(self);
            ui.close();
        }

        response
    }

    pub fn menu_button(self, egui_ctx: &egui::Context) -> egui::Button<'static> {
        let tokens = egui_ctx.tokens();

        let mut button = if let Some(icon) = self.icon() {
            egui::Button::image_and_text(
                icon.as_image()
                    .tint(tokens.label_button_icon_color)
                    .fit_to_exact_size(tokens.small_icon_size),
                self.text(),
            )
        } else {
            egui::Button::new(self.text())
        };

        if let Some(shortcut_text) = self.formatted_kb_shortcut(egui_ctx) {
            button = button.shortcut_text(shortcut_text);
        }

        button
    }

    /// Show name of command and how to activate it
    #[allow(dead_code)]
    pub fn tooltip_ui(self, ui: &mut egui::Ui) {
        let os = ui.ctx().os();

        let (label, details) = self.text_and_tooltip();

        if let Some(shortcut) = self.primary_kb_shortcut() {
            re_ui::Help::new_without_title()
                .control(label, re_ui::IconText::from_keyboard_shortcut(os, shortcut))
                .ui(ui);
        } else {
            ui.label(label);
        }

        ui.set_max_width(220.0);
        ui.label(details);
    }
}

#[test]
fn check_for_clashing_command_shortcuts() {
    fn clashes(a: KeyboardShortcut, b: KeyboardShortcut) -> bool {
        if a.logical_key != b.logical_key {
            return false;
        }

        if a.modifiers.alt != b.modifiers.alt {
            return false;
        }

        if a.modifiers.shift != b.modifiers.shift {
            return false;
        }

        // On Non-Mac, command is interpreted as ctrl!
        (a.modifiers.command || a.modifiers.ctrl) == (b.modifiers.command || b.modifiers.ctrl)
    }

    use strum::IntoEnumIterator as _;

    for a_cmd in CucumberCommand::iter() {
        for a_shortcut in a_cmd.kb_shortcuts() {
            for b_cmd in CucumberCommand::iter() {
                if a_cmd == b_cmd {
                    continue;
                }
                for b_shortcut in b_cmd.kb_shortcuts() {
                    assert!(
                        !clashes(a_shortcut, b_shortcut),
                        "Command '{a_cmd:?}' and '{b_cmd:?}' have overlapping keyboard shortcuts: {:?} vs {:?}",
                        a_shortcut.format(&egui::ModifierNames::NAMES, true),
                        b_shortcut.format(&egui::ModifierNames::NAMES, true),
                    );
                }
            }
        }
    }
}

/// Sender that queues up the execution of a command.
#[derive(Clone)]
pub struct CommandSender(std::sync::mpsc::Sender<CucumberCommand>);

impl CucumberCommandSender for CommandSender {
    /// Send a command to be executed.
    fn send_ui(&self, command: CucumberCommand) {
        // The only way this can fail is if the receiver has been dropped.
        self.0.send(command).ok();
    }
}

/// Receiver for the [`CommandSender`]
pub struct CommandReceiver(std::sync::mpsc::Receiver<CucumberCommand>);

impl CommandReceiver {
    /// Receive a command to be executed if any is queued.
    pub fn recv(&self) -> Option<CucumberCommand> {
        // The only way this can fail (other than being empty)
        // is if the sender has been dropped.
        self.0.try_recv().ok()
    }
}

/// Creates a new command channel.
pub fn command_channel() -> (CommandSender, CommandReceiver) {
    let (sender, receiver) = std::sync::mpsc::channel();
    (CommandSender(sender), CommandReceiver(receiver))
}
