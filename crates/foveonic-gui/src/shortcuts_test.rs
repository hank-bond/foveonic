//! Checks shared shortcut bindings, native clipboard events, and workspace availability.

use super::*;

fn available() -> Availability {
    Availability {
        enabled: true,
        browser: true,
        import: true,
        settings: true,
    }
}
fn key(key: Key, modifiers: Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn bindings_have_no_conflicts_and_keep_navigation_modifiers_separate() {
    for (index, binding) in BINDINGS.iter().enumerate() {
        for modifiers in [
            Modifiers::NONE,
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::COMMAND,
            Modifiers::COMMAND.plus(Modifiers::SHIFT),
        ] {
            if binding.matches(binding.key, modifiers) {
                assert!(!BINDINGS[index + 1..]
                    .iter()
                    .any(|other| other.matches(binding.key, modifiers)));
            }
        }
    }
    let ctx = egui::Context::default();
    let mut input = egui::RawInput {
        events: vec![
            key(Key::ArrowRight, Modifiers::SHIFT),
            key(Key::ArrowLeft, Modifiers::ALT),
            key(Key::ArrowDown, Modifiers::ALT.plus(Modifiers::SHIFT)),
        ],
        ..Default::default()
    };
    let actions = capture(&ctx, &mut input, available());
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].modifiers, Modifiers::SHIFT);
    assert_eq!(actions[1].modifiers, Modifiers::ALT);
    assert_eq!(input.events.len(), 1);
}

#[test]
fn native_clipboard_events_route_only_with_command_shift() {
    for command in [
        Modifiers::COMMAND,
        Modifiers::MAC_CMD.plus(Modifiers::COMMAND),
        Modifiers::CTRL.plus(Modifiers::COMMAND),
    ] {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput {
            modifiers: command.plus(Modifiers::SHIFT),
            events: vec![egui::Event::Copy, egui::Event::Paste("recipe".into())],
            ..Default::default()
        };
        let actions = capture(&ctx, &mut input, available());
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].action, Action::CopySettings);
        assert_eq!(actions[1].action, Action::PasteSettings);
        assert_eq!(actions[1].text.as_deref(), Some("recipe"));
        assert!(input.events.is_empty());
        input.modifiers = command;
        input.events = vec![egui::Event::Copy, egui::Event::Paste("text".into())];
        assert!(capture(&ctx, &mut input, available()).is_empty());
        assert_eq!(input.events.len(), 2);
    }
}

#[test]
fn unavailable_actions_and_unfocused_windows_keep_their_events() {
    let ctx = egui::Context::default();
    let mut input = egui::RawInput {
        modifiers: Modifiers::COMMAND.plus(Modifiers::SHIFT),
        events: vec![
            egui::Event::Copy,
            key(Key::Space, Modifiers::NONE),
            key(Key::ArrowRight, Modifiers::NONE),
        ],
        ..Default::default()
    };
    let mut availability = available();
    availability.settings = false;
    availability.import = false;
    let actions = capture(&ctx, &mut input, availability);
    assert_eq!(actions.len(), 1);
    assert_eq!(input.events.len(), 2);
    input.focused = false;
    assert!(capture(&ctx, &mut input, available()).is_empty());
    assert_eq!(input.events.len(), 2);
}

#[test]
fn text_editors_menus_and_modals_keep_clipboard_commands() {
    let ctx = egui::Context::default();
    let mut value = "text".to_owned();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.text_edit_singleline(&mut value).request_focus();
        });
    });
    let mut input = egui::RawInput {
        modifiers: Modifiers::COMMAND.plus(Modifiers::SHIFT),
        events: vec![egui::Event::Copy, egui::Event::Paste("text".into())],
        ..Default::default()
    };
    assert!(capture(&ctx, &mut input, available()).is_empty());
    assert_eq!(input.events.len(), 2);
    ctx.memory_mut(|memory| {
        if let Some(id) = memory.focused() {
            memory.surrender_focus(id);
        }
        memory.open_popup(egui::Id::new("menu"));
    });
    assert!(capture(&ctx, &mut input, available()).is_empty());
    ctx.memory_mut(|memory| memory.close_popup());
    let _ = ctx.run(Default::default(), |ctx| {
        egui::Modal::new(egui::Id::new("modal")).show(ctx, |ui| {
            ui.label("Modal");
        });
    });
    assert!(capture(&ctx, &mut input, available()).is_empty());
    assert_eq!(input.events.len(), 2);
}
