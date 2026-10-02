use super::Fixture;
use crate::local_asr_lifecycle::{
    apply_worker_transition, choose_worker_transition, OwnedWorkerState,
};
use std::cell::RefCell;

fn check_live_listener_alias(current_endpoint: &str, requested_endpoint: &str, threads: u32) {
    let mut fixture = Fixture::new();
    fixture
        .config
        .speculative
        .streaming_service
        .as_mut()
        .unwrap()
        .endpoint = current_endpoint.into();
    let current = fixture.plan(&fixture.config).unwrap();
    let mut requested = fixture.config.clone();
    let service = requested.speculative.streaming_service.as_mut().unwrap();
    service.endpoint = requested_endpoint.into();
    service.local_daemon.as_mut().unwrap().num_threads = Some(threads);
    let requested_plan = fixture.plan(&requested).unwrap();
    // Use production URL normalization and launch planning, including localhost
    // aliases, path/query changes, and canonical IPv6 addresses.
    assert_eq!(
        current.bind.parse::<std::net::SocketAddr>().unwrap(),
        requested_plan.bind.parse::<std::net::SocketAddr>().unwrap(),
    );
    let state = OwnedWorkerState {
        plan: &current,
        running: true,
    };
    let events = RefCell::new(Vec::new());
    let transition = choose_worker_transition(
        Some(&state),
        requested_endpoint,
        || {
            events.borrow_mut().push("probe owned listener");
            true
        },
        || {
            events.borrow_mut().push("resolve");
            Ok::<_, &str>(Some(requested_plan.clone()))
        },
    )
    .unwrap();
    let mut owned = Some("original owned child");
    assert!(apply_worker_transition(
        transition,
        &mut owned,
        |_| {
            events.borrow_mut().push("stop and wait");
            Ok::<_, &str>(())
        },
        |plan| {
            assert_eq!(plan, requested_plan);
            events.borrow_mut().push("launch");
            Ok("replacement owned child")
        },
    )
    .unwrap());
    if threads == 2 {
        assert_eq!(owned, Some("original owned child"));
        assert_eq!(*events.borrow(), vec!["resolve"]);
    } else {
        assert_eq!(owned, Some("replacement owned child"));
        assert_eq!(*events.borrow(), vec!["resolve", "stop and wait", "launch"]);
    }
}

#[test]
fn live_listener_hostname_alias_keeps_the_owned_worker() {
    check_live_listener_alias("ws://127.0.0.1:53171/asr", "ws://localhost:53171/asr", 2);
}

#[test]
fn live_listener_hostname_alias_applies_changed_thread_count() {
    check_live_listener_alias("ws://127.0.0.1:53171/asr", "ws://localhost:53171/asr", 1);
}

#[test]
fn live_listener_path_and_query_alias_keeps_the_owned_worker() {
    check_live_listener_alias(
        "ws://127.0.0.1:53171/asr",
        "ws://127.0.0.1:53171/other?client=desktop",
        2,
    );
}

#[test]
fn live_listener_ipv6_alias_keeps_the_owned_worker() {
    check_live_listener_alias(
        "ws://[::1]:53171/asr",
        "ws://[0:0:0:0:0:0:0:1]:53171/asr",
        2,
    );
}
