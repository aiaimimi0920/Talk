use talk_core::VoiceEvent;
use talk_hotkey::{HotkeyAction, HotkeyStateMachine};

#[test]
fn toggle_hotkey_alternates_start_and_stop_events() {
    let mut hotkeys = HotkeyStateMachine::new_toggle("Ctrl+Alt+Space");

    assert_eq!(
        hotkeys.handle_action(HotkeyAction::TogglePressed),
        Some(VoiceEvent::TriggerStart)
    );
    assert_eq!(
        hotkeys.handle_action(HotkeyAction::TogglePressed),
        Some(VoiceEvent::TriggerStop)
    );
    assert_eq!(
        hotkeys.handle_action(HotkeyAction::CancelPressed),
        Some(VoiceEvent::TriggerCancel)
    );
}
