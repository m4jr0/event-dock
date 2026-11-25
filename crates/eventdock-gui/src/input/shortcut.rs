use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    OpenTab,
    CloseActiveTab,
}

struct ShortcutBinding {
    command: Command,
    shortcut: egui::KeyboardShortcut,
}

pub(crate) struct ShortcutManager {
    bindings: Vec<ShortcutBinding>,
}

impl Default for ShortcutManager {
    fn default() -> Self {
        Self {
            bindings: vec![
                ShortcutBinding {
                    command: Command::OpenTab,
                    shortcut: egui::KeyboardShortcut::new(
                        egui::Modifiers::COMMAND,
                        egui::Key::T,
                    ),
                },
                ShortcutBinding {
                    command: Command::CloseActiveTab,
                    shortcut: egui::KeyboardShortcut::new(
                        egui::Modifiers::COMMAND,
                        egui::Key::W,
                    ),
                },
            ],
        }
    }
}

impl ShortcutManager {
    pub(crate) fn consume(
        &self,
        ctx: &egui::Context,
    ) -> Option<Command> {
        ctx.input_mut(|input| {
            self.bindings
                .iter()
                .find(|binding| input.consume_shortcut(&binding.shortcut))
                .map(|binding| binding.command)
        })
    }

    pub(crate) fn label(
        &self,
        ctx: &egui::Context,
        command: Command,
    ) -> String {
        self.bindings
            .iter()
            .find(|binding| binding.command == command)
            .map(|binding| ctx.format_shortcut(&binding.shortcut))
            .unwrap_or_default()
    }
}
