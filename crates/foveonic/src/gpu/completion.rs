//! Services completion notifications while parking the caller outside wgpu's device locks.

use crate::source::Result;
use std::{sync::mpsc, time::Duration};

// A render waits many times, once per tick, so a coarse interval adds up.
const POLL_INTERVAL: Duration = Duration::from_micros(100);

/// The callback captures work submitted before registration, including any UI
/// submission that raced with it. Submissions after registration do not extend it.
pub(super) fn submitted(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
    wait(device, marker(queue), || Ok(()))
}

/// Signals once the work submitted so far has finished.
pub(super) fn marker(queue: &wgpu::Queue) -> mpsc::Receiver<()> {
    let (sender, receiver) = mpsc::channel();
    queue.on_submitted_work_done(move || {
        let _ = sender.send(());
    });
    receiver
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
