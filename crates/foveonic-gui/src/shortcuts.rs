//! Defines workspace actions and routes their keys before widgets, preserving text and modal input.

use eframe::egui::{self, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserAction {
    SelectAll,
    TogglePick,
    Hide,
    Arrow(Key),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Browser(BrowserAction),
    CopySettings,
    PasteSettings,
}

pub struct Invocation {
    pub action: Action,
    pub modifiers: Modifiers,
    pub text: Option<String>,
}

#[derive(Clone, Copy)]
enum ModifierRule {
    Exact(Modifiers),
    Navigation,
}

struct Binding {
    key: Key,
    modifiers: ModifierRule,
    action: Action,
}
impl Binding {
    const fn new(key: Key, modifiers: Modifiers, action: Action) -> Self {
        Self {
            key,
            modifiers: ModifierRule::Exact(modifiers),
            action,
        }
    }
    const fn arrow(key: Key) -> Self {
        Self {
            key,
            modifiers: ModifierRule::Navigation,
            action: Action::Browser(BrowserAction::Arrow(key)),
        }
    }
    fn matches(&self, key: Key, modifiers: Modifiers) -> bool {
        self.key == key
            && match self.modifiers {
                ModifierRule::Exact(expected) => modifiers.matches_exact(expected),
                ModifierRule::Navigation => {
                    modifiers.is_none()
                        || modifiers.shift_only()
                        || modifiers.matches_exact(Modifiers::ALT)
                }
            }
    }
}

const BINDINGS: &[Binding] = &[
    Binding::new(
        Key::A,
        Modifiers::COMMAND,
        Action::Browser(BrowserAction::SelectAll),
    ),
    Binding::new(
        Key::Space,
        Modifiers::NONE,
        Action::Browser(BrowserAction::TogglePick),
    ),
    Binding::new(
        Key::Delete,
        Modifiers::NONE,
        Action::Browser(BrowserAction::Hide),
    ),
    Binding::new(
        Key::Backspace,
        Modifiers::NONE,
        Action::Browser(BrowserAction::Hide),
    ),
    Binding::arrow(Key::ArrowLeft),
    Binding::arrow(Key::ArrowRight),
    Binding::arrow(Key::ArrowUp),
    Binding::arrow(Key::ArrowDown),
    Binding::new(
        Key::C,
        Modifiers::COMMAND.plus(Modifiers::SHIFT),
        Action::CopySettings,
    ),
    Binding::new(
        Key::V,
        Modifiers::COMMAND.plus(Modifiers::SHIFT),
        Action::PasteSettings,
    ),
];

pub struct Availability {
    pub enabled: bool,
    pub browser: bool,
    pub import: bool,
    pub settings: bool,
}
impl Availability {
    fn allows(&self, action: Action) -> bool {
        self.enabled
            && match action {
                Action::Browser(BrowserAction::TogglePick | BrowserAction::Hide) => {
                    self.browser && self.import
                }
                Action::Browser(_) => self.browser,
                Action::CopySettings | Action::PasteSettings => self.settings,
            }
    }
}

/// Capture only workspace commands. Widget-local editing keys stay with egui.
pub fn capture(
    ctx: &egui::Context,
    input: &mut egui::RawInput,
    available: Availability,
) -> Vec<Invocation> {
    let mut actions = Vec::new();
    if !available.enabled || !input.focused {
        return actions;
    }
    let (focused, modal) = ctx.memory(|memory| {
        (
            memory.focused(),
            memory.any_popup_open() || memory.top_modal_layer().is_some(),
        )
    });
    // Load widget state after releasing egui's context lock. DragValue edits
    // use TextEdit with the same focused ID.
    let typing = focused.is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
    if typing || modal {
        return actions;
    }
    input.events.retain(|event| {
        // egui-winit emits clipboard events instead of C/V key presses, even
        // with Shift held. Retain their payload and leave plain text copy/paste alone.
        let (key, modifiers, text) = match event {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => (*key, *modifiers, None),
            egui::Event::Copy => (Key::C, input.modifiers, None),
            egui::Event::Paste(text) => (Key::V, input.modifiers, Some(text)),
            _ => return true,
        };
        if let Some(binding) = BINDINGS
            .iter()
            .find(|b| b.matches(key, modifiers) && available.allows(b.action))
        {
            actions.push(Invocation {
                action: binding.action,
                modifiers,
                text: text.cloned(),
            });
            false
        } else {
            true
        }
    });
    actions
}

#[cfg(test)]
#[path = "shortcuts_test.rs"]
mod tests;
