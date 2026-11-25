//! Orders submitted jobs by priority tier, then by rank, then by when each target joined the line.

use super::scheduler::Job;
use crate::priority::Priority;

pub(super) struct Queued<J> {
    pub job: J,
    /// When the target joined the line. Replacing a queued job keeps it.
    pub order: u64,
    /// Waiting for an input the host is acquiring.
    pub waiting: bool,
    /// Did not fit or could not start. It retries after a job finishes or retires,
    /// memory is freed, or the limit changes.
    pub deferred: bool,
}

pub(super) struct Queue<J> {
    entries: Vec<Queued<J>>,
    next_order: u64,
}

impl<J: Job> Queue<J> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_order: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn jobs(&self) -> impl Iterator<Item = &J> {
        self.entries.iter().map(|queued| &queued.job)
    }

    /// A newer job for a queued target replaces it and keeps its place in line.
    /// Any other job joins the back, including one for a target that is running,
    /// so targets still waiting are served before that target runs again.
    pub fn submit(&mut self, job: J) {
        if let Some(queued) = self
            .entries
            .iter_mut()
            .find(|queued| queued.job.target() == job.target())
        {
            queued.job = job;
            queued.waiting = false;
            queued.deferred = false;
            return;
        }
        self.next_order += 1;
        self.entries.push(Queued {
            job,
            order: self.next_order,
            waiting: false,
            deferred: false,
        });
    }

    /// Return a started job to its place, unless a newer job already replaced it.
    pub fn put_back(&mut self, queued: Queued<J>) {
        if !self.contains(queued.job.target()) {
            self.entries.push(queued);
        }
    }

    pub fn contains(&self, target: &J::Target) -> bool {
        self.entries
            .iter()
            .any(|queued| queued.job.target() == target)
    }

    pub fn update(&mut self, mut update: impl FnMut(&mut J)) {
        for queued in &mut self.entries {
            update(&mut queued.job);
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&J) -> bool) {
        self.entries.retain(|queued| keep(&queued.job));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// The highest tier with queued work. A job waiting for its input still
    /// holds back lower tiers.
    pub fn best_tier(&self) -> Option<Priority> {
        self.entries
            .iter()
            .filter(|queued| !queued.deferred)
            .map(|queued| queued.job.purpose().priority())
            .min()
    }

    /// The first job of this tier, by rank then submission order, that can start now.
    pub fn next(
        &self,
        tier: Priority,
        can_start: impl Fn(&Queued<J>) -> bool,
    ) -> Option<&Queued<J>> {
        self.entries
            .iter()
            .filter(|queued| {
                !queued.deferred && queued.job.purpose().priority() == tier && can_start(queued)
            })
            .min_by_key(|queued| (queued.job.rank(), queued.order))
    }

    pub fn take(&mut self, target: &J::Target) -> Option<Queued<J>> {
        let index = self
            .entries
            .iter()
            .position(|queued| queued.job.target() == target)?;
        Some(self.entries.remove(index))
    }

    pub fn memory_changed(&mut self) {
        for queued in &mut self.entries {
            queued.deferred = false;
        }
    }
}

#[cfg(test)]
#[path = "queue_test.rs"]
mod tests;
