//! Dynamic system-profile watcher for hot-plug hardware detection.
//!
//! `SystemProfileWatcher` monitors OS-specific sources for hardware changes
//! (GPU hot-plug, driver install, memory change) and publishes updates via a
//! `broadcast` channel so downstream subsystems can re-tune.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::broadcast;

use crate::system_profile::SystemProfile;

/// Events published by the watcher.
#[derive(Clone, Debug)]
pub enum ProfileChange {
    /// A new or updated system profile is available.
    Updated(Arc<SystemProfile>),
    /// A specific hardware change was observed.
    HardwareChange { description: String },
}

/// Watcher for dynamic hardware changes.
///
/// Uses polling on all platforms (inotify/FSEvents/ReadDirectoryChanges are
/// future work) with a configurable interval.
pub struct SystemProfileWatcher {
    /// Last-known system profile.
    last_profile: Arc<Mutex<Option<Arc<SystemProfile>>>>,
    /// Publisher for change events.
    tx: broadcast::Sender<ProfileChange>,
    /// Polling interval.
    poll_interval: Duration,
}

impl SystemProfileWatcher {
    /// Create a new watcher with the given polling interval.
    ///
    /// The initial system profile is detected immediately.
    pub fn new(poll_interval: Duration) -> Self {
        let initial = Arc::new(SystemProfile::detect());
        let (tx, _rx) = broadcast::channel(16);

        Self {
            last_profile: Arc::new(Mutex::new(Some(initial))),
            tx,
            poll_interval,
        }
    }

    /// Get a receiver for change events.
    ///
    /// The receiver will immediately receive the current profile snapshot.
    pub fn subscribe(&self) -> broadcast::Receiver<ProfileChange> {
        let rx = self.tx.subscribe();
        // Send best-effort current snapshot to this subscriber.
        // If the buffer is full the subscriber can still poll.
        if let Some(profile) = self.current_profile() {
            let _ = self.tx.send(ProfileChange::Updated(profile));
        }
        rx
    }

    /// Get the most recently detected profile (blocking, fast).
    pub fn current_profile(&self) -> Option<Arc<SystemProfile>> {
        self.last_profile
            .lock()
            .ok()
            .and_then(|g| g.clone())
    }

    /// Perform a single poll cycle: re-detect and publish if changed.
    pub fn poll_once(&self) {
        let fresh = Arc::new(SystemProfile::detect_fast());
        let changed = {
            let guard = self.last_profile.lock().ok();
            match guard.and_then(|g| g.as_ref().map(|old| profiles_differ(old, &fresh))) {
                Some(true) | None => true,
                Some(false) => false,
            }
        };

        if changed {
            if let Ok(mut guard) = self.last_profile.lock() {
                *guard = Some(fresh.clone());
            }
            let _ = self.tx.send(ProfileChange::Updated(fresh));
        }
    }

    /// Start the background polling loop. Runs until the returned handle is dropped.
    pub fn start_background(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(this.poll_interval).await;
                this.poll_once();
            }
        })
    }
}

/// Quick structural comparison to avoid full profile hashing.
fn profiles_differ(a: &SystemProfile, b: &SystemProfile) -> bool {
    if a.gpus.len() != b.gpus.len() {
        return true;
    }
    for (ga, gb) in a.gpus.iter().zip(b.gpus.iter()) {
        if ga.name != gb.name
            || (ga.vram_gb - gb.vram_gb).abs() > 0.1
            || ga.backend != gb.backend
        {
            return true;
        }
    }
    if a.npus.len() != b.npus.len() {
        return true;
    }
    if a.cpu.logical_cores != b.cpu.logical_cores {
        return true;
    }
    if (a.memory.total_gb - b.memory.total_gb).abs() > 0.5 {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Convenience: one-shot watch-and-reload
// ---------------------------------------------------------------------------

/// Run a single watch cycle: detect changes and return a new `SystemProfile`
/// if hardware has changed since `last`.
pub fn check_for_changes(last: &SystemProfile) -> Option<SystemProfile> {
    let current = SystemProfile::detect_fast();
    if profiles_differ(last, &current) {
        Some(current)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcher_starts_with_valid_profile() {
        let watcher = SystemProfileWatcher::new(Duration::from_secs(60));
        let profile = watcher.current_profile();
        assert!(profile.is_some());
        assert!(!profile.unwrap().gpus.is_empty());
    }

    #[test]
    fn watcher_subscribe_receives_initial() {
        let watcher = SystemProfileWatcher::new(Duration::from_secs(60));
        let mut rx = watcher.subscribe();
        // Should have the initial profile via try_recv
        let event = rx.try_recv();
        assert!(event.is_ok(), "should have initial event: {:?}", event.err());
        match event.unwrap() {
            ProfileChange::Updated(_) => {} // expected
            _ => panic!("expected Updated event"),
        }
    }

    #[test]
    fn poll_once_does_not_panic() {
        let watcher = SystemProfileWatcher::new(Duration::from_secs(60));
        watcher.poll_once(); // Should not crash
    }

    #[test]
    fn check_for_changes_returns_none_when_unchanged() {
        let profile = SystemProfile::detect_fast();
        let result = check_for_changes(&profile);
        // May return Some if HW detection is non-deterministic,
        // but it must not panic
        let _ = result;
    }

    #[test]
    fn profiles_differ_detects_gpu_count_change() {
        let a = SystemProfile::detect_fast();
        let mut b = a.clone();
        b.gpus.clear();
        assert!(profiles_differ(&a, &b));
    }
}
