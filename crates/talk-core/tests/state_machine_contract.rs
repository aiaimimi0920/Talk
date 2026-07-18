use talk_core::{SessionStatus, VoiceEvent, VoiceSession};

#[test]
fn toggle_recording_session_reaches_completed_state() {
    let mut session = VoiceSession::new_for_test("session-1");

    assert_eq!(session.status(), SessionStatus::Idle);

    session
        .apply(VoiceEvent::TriggerStart)
        .expect("start accepted");
    assert_eq!(session.status(), SessionStatus::Recording);

    session
        .apply(VoiceEvent::TriggerStop)
        .expect("stop accepted");
    assert_eq!(session.status(), SessionStatus::Transcribing);

    session
        .apply(VoiceEvent::TranscriptReady {
            text: "hello neuro".to_string(),
        })
        .expect("transcript accepted");
    assert_eq!(session.status(), SessionStatus::Processing);

    session
        .apply(VoiceEvent::ProcessedTextReady {
            text: "Hello, Neuro.".to_string(),
        })
        .expect("processed text accepted");
    assert_eq!(session.status(), SessionStatus::Inserting);

    session
        .apply(VoiceEvent::InsertSucceeded)
        .expect("insert accepted");
    assert_eq!(session.status(), SessionStatus::Completed);
    assert_eq!(session.transcript(), Some("hello neuro"));
    assert_eq!(session.output_text(), Some("Hello, Neuro."));
}

#[test]
fn cancel_from_active_state_is_terminal() {
    let mut session = VoiceSession::new_for_test("session-2");

    session
        .apply(VoiceEvent::TriggerStart)
        .expect("start accepted");
    session
        .apply(VoiceEvent::TriggerCancel)
        .expect("cancel accepted");

    assert_eq!(session.status(), SessionStatus::Cancelled);
    let error = session
        .apply(VoiceEvent::TriggerStart)
        .expect_err("cancelled session is terminal");
    assert!(error.to_string().contains("terminal"));
}

#[test]
fn invalid_transition_reports_current_state() {
    let mut session = VoiceSession::new_for_test("session-3");

    let error = session
        .apply(VoiceEvent::TriggerStop)
        .expect_err("cannot stop idle session");
    let message = error.to_string();

    assert!(message.contains("Idle"));
    assert!(message.contains("TriggerStop"));
}
