use std::net::SocketAddr;
use talk_desktop::{desktop_local_asr_daemon_bind_from_endpoint, DesktopLocalAsrDaemonLaunchPlan};

pub(crate) struct OwnedWorkerState<'a> {
    pub plan: &'a DesktopLocalAsrDaemonLaunchPlan,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkerTransition {
    ReuseOwned,
    ConnectExternal,
    ReplaceOwned(Option<DesktopLocalAsrDaemonLaunchPlan>),
}

pub(crate) fn choose_worker_transition<E>(
    owned: Option<&OwnedWorkerState<'_>>,
    requested_endpoint: &str,
    probe_endpoint: impl FnOnce() -> bool,
    resolve_plan: impl FnOnce() -> Result<Option<DesktopLocalAsrDaemonLaunchPlan>, E>,
) -> Result<WorkerTransition, E> {
    let requested_bind = desktop_local_asr_daemon_bind_from_endpoint(requested_endpoint)
        .ok()
        .flatten()
        .and_then(|bind| bind.parse::<SocketAddr>().ok());
    // Compare the actual listener, not the URI spelling: localhost, IPv6
    // aliases, and WebSocket paths can still reach our own live child.
    let live_at_endpoint = owned.filter(|worker| {
        worker.running
            && requested_bind
                .is_some_and(|bind| worker.plan.bind.parse::<SocketAddr>().ok() == Some(bind))
    });
    // A dead owned child or a different endpoint can now be served by an
    // external process. Do not require packaged files to connect to it.
    if live_at_endpoint.is_none() && probe_endpoint() {
        return Ok(WorkerTransition::ConnectExternal);
    }
    let requested_plan = resolve_plan()?;
    if live_at_endpoint.is_some_and(|worker| requested_plan.as_ref() == Some(worker.plan)) {
        return Ok(WorkerTransition::ReuseOwned);
    }
    Ok(WorkerTransition::ReplaceOwned(requested_plan))
}

pub(crate) fn stop_owned_worker<T, E>(
    owned: &mut Option<T>,
    stop: impl FnOnce(&mut T) -> Result<(), E>,
) -> Result<(), E> {
    if let Some(worker) = owned.as_mut() {
        stop(worker)?;
    }
    // Keep ownership if killing or waiting failed, so a replacement cannot
    // race that child and a later cleanup can still address the owned process.
    *owned = None;
    Ok(())
}

pub(crate) fn apply_worker_transition<T, E>(
    transition: WorkerTransition,
    owned: &mut Option<T>,
    stop: impl FnOnce(&mut T) -> Result<(), E>,
    launch: impl FnOnce(DesktopLocalAsrDaemonLaunchPlan) -> Result<T, E>,
) -> Result<bool, E> {
    match transition {
        WorkerTransition::ReuseOwned => Ok(true),
        WorkerTransition::ConnectExternal => {
            stop_owned_worker(owned, stop)?;
            Ok(true)
        }
        WorkerTransition::ReplaceOwned(plan) => {
            stop_owned_worker(owned, stop)?;
            if let Some(plan) = plan {
                *owned = Some(launch(plan)?);
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests;
