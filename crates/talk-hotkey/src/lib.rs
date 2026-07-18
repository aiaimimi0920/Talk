use serde::{Deserialize, Serialize};
use talk_core::VoiceEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotkeyAction {
    TogglePressed,
    PushToTalkPressed,
    PushToTalkReleased,
    CancelPressed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyStateMachine {
    shortcut: String,
    recording: bool,
}

impl HotkeyStateMachine {
    pub fn new_toggle(shortcut: impl Into<String>) -> Self {
        Self {
            shortcut: shortcut.into(),
            recording: false,
        }
    }

    pub fn shortcut(&self) -> &str {
        &self.shortcut
    }

    pub fn handle_action(&mut self, action: HotkeyAction) -> Option<VoiceEvent> {
        match action {
            HotkeyAction::TogglePressed => {
                self.recording = !self.recording;
                if self.recording {
                    Some(VoiceEvent::TriggerStart)
                } else {
                    Some(VoiceEvent::TriggerStop)
                }
            }
            HotkeyAction::PushToTalkPressed if !self.recording => {
                self.recording = true;
                Some(VoiceEvent::TriggerStart)
            }
            HotkeyAction::PushToTalkReleased if self.recording => {
                self.recording = false;
                Some(VoiceEvent::TriggerStop)
            }
            HotkeyAction::CancelPressed => {
                self.recording = false;
                Some(VoiceEvent::TriggerCancel)
            }
            _ => None,
        }
    }
}
