//! Services completion notifications while parking the caller outside wgpu's device locks.

use crate::source::Result;
use std::{sync::mpsc, time::Duration};

const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// The callback captures work submitted before registration, including any UI
/// submission that raced with it. Submissions after registration do not extend it.
pub(super) fn submitted(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
    let (sender, receiver) = mpsc::channel();
    queue.on_submitted_work_done(move || {
        let _ = sender.send(());
    });
    wait(device, receiver, || Ok(()))
}

/// The caller supplies its existing interruption policy. Submitted-stage waits
/// drain fully before cancellation, while speculative readback may be interrupted.
pub(super) fn wait<T>(
    device: &wgpu::Device,
    receiver: mpsc::Receiver<T>,
    mut check: impl FnMut() -> Result<()>,
) -> Result<T> {
    loop {
        check()?;
        device.poll(wgpu::PollType::Poll).map_err(|error| {
            super::failure("poll-completion", error.into(), serde_json::Value::Null)
        })?;
        match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(value) => return Ok(value),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(error) => {
                return Err(super::failure(
                    "completion-channel",
                    error.into(),
                    serde_json::Value::Null,
                ))
            }
        }
    }
}

#[cfg(test)]
#[path = "completion_test.rs"]
mod tests;
