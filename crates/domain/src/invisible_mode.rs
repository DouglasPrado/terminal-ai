//! Invisible mode: the pure state and timing rules behind hiding the app from screen capture.
//!
//! Nothing here touches the operating system. The switches themselves live in the composition
//! root, because a window handle only exists there; what lives here is everything that can be
//! decided without one — and therefore everything that can be tested without one.

use std::time::{Duration, Instant};

/// How long macOS needs between making the app visible in the Dock again and hiding it once more.
///
/// This is not a guess. `tao`'s dock helper makes `set_dock_hide` a no-op when it lands within one
/// second of a `set_dock_show`, because the process-type transition is asynchronous with no
/// completion signal and a rapid hide→show→hide leaves duplicate Dock icons stuck in the system.
/// A caller that ignores this gets a hide that is silently dropped: the Dock icon stays while the
/// UI says the app is hidden.
pub const DOCK_TRANSITION_COOLDOWN: Duration = Duration::from_millis(1_000);

/// The whole feature's state.
///
/// A newtype rather than a bare `bool` so the transition rules have somewhere to live, and so a
/// call site cannot silently pass the wrong boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InvisibleMode {
    pub enabled: bool,
}

impl InvisibleMode {
    pub const fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Whether moving to `target` is a real change. Re-applying the current state is a no-op, not
    /// an error: the user asking for what they already have has succeeded.
    pub const fn changes_to(self, target: bool) -> bool {
        self.enabled != target
    }
}

/// Tracks the last time the app made itself visible in the Dock, so a later hide can wait out the
/// cooldown above instead of being swallowed.
#[derive(Debug, Clone, Copy, Default)]
pub struct DockCooldown {
    last_show: Option<Instant>,
}

impl DockCooldown {
    pub const fn new() -> Self {
        Self { last_show: None }
    }

    /// Record that the app just became visible in the Dock.
    pub fn record_show(&mut self, at: Instant) {
        self.last_show = Some(at);
    }

    /// Record that the app just left the Dock. A hide does not start a cooldown — only a show
    /// does — so this clears the tracker rather than setting it.
    pub fn record_hide(&mut self) {
        self.last_show = None;
    }

    /// How long a hide requested at `now` must wait to actually take effect.
    ///
    /// Zero when no show is pending, and zero once the full cooldown has elapsed. `now` is a
    /// parameter rather than a clock read so the boundaries are testable without sleeping.
    pub fn remaining(&self, now: Instant) -> Duration {
        match self.last_show {
            None => Duration::ZERO,
            Some(last) => {
                let elapsed = now.saturating_duration_since(last);
                DOCK_TRANSITION_COOLDOWN.saturating_sub(elapsed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn re_applying_the_current_state_is_not_a_change() {
        assert!(!InvisibleMode::new(true).changes_to(true));
        assert!(!InvisibleMode::new(false).changes_to(false));
        assert!(InvisibleMode::new(false).changes_to(true));
        assert!(InvisibleMode::new(true).changes_to(false));
    }

    #[test]
    fn default_is_visible() {
        assert!(!InvisibleMode::default().enabled);
    }

    #[test]
    fn no_previous_show_means_no_wait() {
        let cooldown = DockCooldown::new();
        assert_eq!(cooldown.remaining(Instant::now()), Duration::ZERO);
    }

    #[test]
    fn a_hide_immediately_after_a_show_waits_the_whole_cooldown() {
        let now = Instant::now();
        let mut cooldown = DockCooldown::new();
        cooldown.record_show(now);
        assert_eq!(cooldown.remaining(now), DOCK_TRANSITION_COOLDOWN);
    }

    #[test]
    fn waiting_is_what_is_left_of_the_cooldown() {
        let now = Instant::now();
        let mut cooldown = DockCooldown::new();
        cooldown.record_show(now);
        let after_999ms = now + Duration::from_millis(999);
        assert_eq!(cooldown.remaining(after_999ms), Duration::from_millis(1));
    }

    #[test]
    fn the_boundary_itself_needs_no_wait() {
        let now = Instant::now();
        let mut cooldown = DockCooldown::new();
        cooldown.record_show(now);
        assert_eq!(
            cooldown.remaining(now + DOCK_TRANSITION_COOLDOWN),
            Duration::ZERO
        );
    }

    #[test]
    fn past_the_boundary_never_goes_negative() {
        let now = Instant::now();
        let mut cooldown = DockCooldown::new();
        cooldown.record_show(now);
        assert_eq!(
            cooldown.remaining(now + Duration::from_secs(30)),
            Duration::ZERO
        );
    }

    #[test]
    fn hiding_clears_the_pending_cooldown() {
        let now = Instant::now();
        let mut cooldown = DockCooldown::new();
        cooldown.record_show(now);
        cooldown.record_hide();
        assert_eq!(cooldown.remaining(now), Duration::ZERO);
    }
}
