//! Kernel lifecycle: what state the memory kernel is in, and what the app may do about it.
//!
//! The decision logic is a pure function ([`transition`]) with no IO, for one reason above all
//! others: Constitution VII says the app may supervise only a process it started itself. A user
//! running their own `ai-memory` — via Docker, launchd or `mise`, on a store the app *shares* —
//! must not have it killed when the app quits. That is a data-loss-class rule, and a rule like
//! that belongs in something a test can exhaust rather than in a scenario someone remembers to run.

use std::time::Duration;

/// Where the kernel is, from the app's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Nothing has been attempted yet.
    Idle,
    Probing,
    /// No binary could be resolved anywhere. Terminal.
    NotInstalled,
    Starting,
    /// Running, started by us. The only state in which we may stop or restart it.
    Ready,
    /// Running, started by someone else.
    Attached,
    /// Was reachable, is not answering. Will be retried.
    Degraded,
    /// Something holds the port and it is not a kernel. Terminal until the user acts.
    PortConflict,
    /// Gave up after repeated failures. Terminal until the user acts.
    Failed,
}

/// Something that happened, or that the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Start,
    BinaryMissing,
    /// A kernel answered at the configured address, and we did not start it.
    ProbeFoundForeignKernel,
    /// A kernel answered, and it is the process we started (adopted via the pidfile).
    ProbeFoundOwnKernel,
    /// Something answered, but it is not a kernel.
    ProbeFoundStranger,
    /// Nothing is listening.
    ProbeRefused,
    SpawnSucceeded,
    SpawnFailed,
    /// The spawned child died immediately — usually Gatekeeper quarantine.
    SpawnKilledBySignal,
    ReadyConfirmed,
    StartupTimedOut,
    /// A health probe failed while we believed the kernel was up.
    HealthCheckFailed,
    BackoffElapsed,
    UserRequestedStart,
    UserRequestedStop,
    UserRequestedRestart,
    AppExiting,
}

/// What the caller should actually do. The machine decides; the caller performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Probe,
    Spawn,
    /// Send SIGTERM, then SIGKILL. **Only ever emitted for a process we started.**
    Terminate,
    ScheduleBackoff(Duration),
    /// Ask the user to clear the Gatekeeper quarantine.
    ReportQuarantine,
}

/// The supervisor's whole decision state. Small on purpose: everything else is IO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    pub state: State,
    /// Set only when *we* spawned the process. The single gate on [`Action::Terminate`].
    pub owned: bool,
    pub consecutive_failures: u32,
}

impl Default for Machine {
    fn default() -> Self {
        Self {
            state: State::Idle,
            owned: false,
            consecutive_failures: 0,
        }
    }
}

/// After this many consecutive failures we stop retrying and wait for the user. A kernel that
/// cannot start will not start on the twentieth attempt either, and a spawn loop is worse than an
/// honest error.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Exponential backoff, capped. Jitter is applied by the caller, which owns the clock.
#[must_use]
pub fn backoff_for(attempt: u32) -> Duration {
    const CAP_SECS: u64 = 30;
    let secs = 1u64
        .checked_shl(attempt.min(16))
        .unwrap_or(CAP_SECS)
        .min(CAP_SECS);
    Duration::from_secs(secs)
}

/// The whole decision. Pure: no clock, no network, no process handling.
#[must_use]
pub fn transition(machine: Machine, event: Event) -> (Machine, Vec<Action>) {
    use Event as E;
    use State as S;

    let mut next = machine;

    match (machine.state, event) {
        // ---- Starting up -------------------------------------------------------------------
        (
            S::Idle | S::NotInstalled | S::PortConflict | S::Failed,
            E::Start | E::UserRequestedStart,
        ) => {
            next.state = S::Probing;
            next.consecutive_failures = 0;
            (next, vec![Action::Probe])
        }

        (S::Probing, E::BinaryMissing) => {
            next.state = S::NotInstalled;
            (next, vec![])
        }

        // Someone else's server. We use it and we never manage it.
        (S::Probing | S::Degraded, E::ProbeFoundForeignKernel) => {
            next.state = S::Attached;
            next.owned = false;
            next.consecutive_failures = 0;
            (next, vec![])
        }

        // Our own process, adopted after a crash or reload rather than spawning a second one.
        (S::Probing | S::Degraded | S::Starting, E::ProbeFoundOwnKernel | E::ReadyConfirmed) => {
            next.state = S::Ready;
            next.owned = true;
            next.consecutive_failures = 0;
            (next, vec![])
        }

        // Something is on the port that is not a kernel. Do not attach; do not spawn over it.
        (S::Probing | S::Degraded | S::Starting, E::ProbeFoundStranger) => {
            next.state = S::PortConflict;
            next.owned = false;
            (next, vec![])
        }

        (S::Probing, E::ProbeRefused) => {
            next.state = S::Starting;
            (next, vec![Action::Spawn])
        }

        (S::Starting, E::SpawnSucceeded) => (next, vec![Action::Probe]),

        (S::Starting, E::SpawnKilledBySignal) => {
            next.state = S::Failed;
            next.owned = false;
            (next, vec![Action::ReportQuarantine])
        }

        (S::Starting, E::SpawnFailed | E::StartupTimedOut) => fail_or_retry(next),

        // ---- Losing it ---------------------------------------------------------------------
        (S::Ready, E::HealthCheckFailed) => {
            next.state = S::Degraded;
            fail_or_retry(next)
        }

        // An attached server going quiet is not our failure to recover from. Re-probe from
        // scratch: it may have been restarted, replaced, or genuinely stopped by its owner.
        (S::Attached, E::HealthCheckFailed) => {
            next.state = S::Probing;
            next.owned = false;
            (next, vec![Action::Probe])
        }

        (S::Degraded, E::BackoffElapsed) => {
            next.state = S::Probing;
            (next, vec![Action::Probe])
        }

        (S::Degraded, E::ProbeRefused) => {
            next.state = S::Starting;
            (next, vec![Action::Spawn])
        }

        // ---- User actions ------------------------------------------------------------------
        //
        // These two arms are the reason this module exists. `owned` is checked here, once, rather
        // than at each call site.
        (_, E::UserRequestedStop) => {
            if machine.owned {
                next.state = S::Idle;
                next.owned = false;
                (next, vec![Action::Terminate])
            } else {
                (next, vec![])
            }
        }

        (_, E::UserRequestedRestart) => {
            if machine.owned {
                next.state = S::Probing;
                next.owned = false;
                next.consecutive_failures = 0;
                (next, vec![Action::Terminate, Action::Probe])
            } else {
                (next, vec![])
            }
        }

        (_, E::AppExiting) => {
            if machine.owned {
                next.state = S::Idle;
                next.owned = false;
                (next, vec![Action::Terminate])
            } else {
                next.state = S::Idle;
                (next, vec![])
            }
        }

        // Anything else is a no-op rather than a panic: an unexpected event should not take the
        // app down, and the next probe re-synchronises reality.
        _ => (next, vec![]),
    }
}

fn fail_or_retry(mut machine: Machine) -> (Machine, Vec<Action>) {
    machine.consecutive_failures = machine.consecutive_failures.saturating_add(1);
    machine.owned = false;
    if machine.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
        machine.state = State::Failed;
        (machine, vec![])
    } else {
        machine.state = State::Degraded;
        let delay = backoff_for(machine.consecutive_failures - 1);
        (machine, vec![Action::ScheduleBackoff(delay)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(start: Machine, events: &[Event]) -> (Machine, Vec<Action>) {
        let mut machine = start;
        let mut actions = Vec::new();
        for event in events {
            let (next, mut emitted) = transition(machine, *event);
            machine = next;
            actions.append(&mut emitted);
        }
        (machine, actions)
    }

    #[test]
    fn cold_start_probes_then_spawns_then_becomes_ready() {
        let (machine, actions) = drive(
            Machine::default(),
            &[
                Event::Start,
                Event::ProbeRefused,
                Event::SpawnSucceeded,
                Event::ReadyConfirmed,
            ],
        );
        assert_eq!(machine.state, State::Ready);
        assert!(machine.owned, "a kernel we spawned is ours to manage");
        assert_eq!(actions, vec![Action::Probe, Action::Spawn, Action::Probe]);
    }

    #[test]
    fn a_foreign_kernel_is_attached_not_managed() {
        let (machine, actions) = drive(
            Machine::default(),
            &[Event::Start, Event::ProbeFoundForeignKernel],
        );
        assert_eq!(machine.state, State::Attached);
        assert!(!machine.owned);
        assert!(
            !actions.contains(&Action::Spawn),
            "must not start a second server"
        );
    }

    /// The rule this whole module exists to enforce.
    #[test]
    fn a_kernel_we_did_not_start_is_never_terminated() {
        let attached = Machine {
            state: State::Attached,
            owned: false,
            consecutive_failures: 0,
        };
        for event in [
            Event::UserRequestedStop,
            Event::UserRequestedRestart,
            Event::AppExiting,
            Event::HealthCheckFailed,
        ] {
            let (_, actions) = transition(attached, event);
            assert!(
                !actions.contains(&Action::Terminate),
                "event {event:?} must not terminate a server we did not start"
            );
        }
    }

    /// The same rule, stated exhaustively: no reachable state with `owned == false` may ever
    /// produce a Terminate, whatever happens to it.
    #[test]
    fn terminate_is_unreachable_for_any_unowned_state() {
        let states = [
            State::Idle,
            State::Probing,
            State::NotInstalled,
            State::Starting,
            State::Ready,
            State::Attached,
            State::Degraded,
            State::PortConflict,
            State::Failed,
        ];
        let events = [
            Event::Start,
            Event::BinaryMissing,
            Event::ProbeFoundForeignKernel,
            Event::ProbeFoundOwnKernel,
            Event::ProbeFoundStranger,
            Event::ProbeRefused,
            Event::SpawnSucceeded,
            Event::SpawnFailed,
            Event::SpawnKilledBySignal,
            Event::ReadyConfirmed,
            Event::StartupTimedOut,
            Event::HealthCheckFailed,
            Event::BackoffElapsed,
            Event::UserRequestedStart,
            Event::UserRequestedStop,
            Event::UserRequestedRestart,
            Event::AppExiting,
        ];
        for state in states {
            for event in events {
                let machine = Machine {
                    state,
                    owned: false,
                    consecutive_failures: 0,
                };
                let (_, actions) = transition(machine, event);
                assert!(
                    !actions.contains(&Action::Terminate),
                    "{state:?} + {event:?} emitted Terminate with owned == false"
                );
            }
        }
    }

    #[test]
    fn a_stranger_on_the_port_neither_attaches_nor_spawns() {
        let (machine, actions) = drive(
            Machine::default(),
            &[Event::Start, Event::ProbeFoundStranger],
        );
        assert_eq!(machine.state, State::PortConflict);
        assert!(!machine.owned);
        assert!(!actions.contains(&Action::Spawn));
    }

    #[test]
    fn repeated_failures_stop_retrying_instead_of_looping() {
        let mut machine = Machine {
            state: State::Starting,
            owned: false,
            consecutive_failures: 0,
        };
        let mut backoffs = Vec::new();
        for _ in 0..MAX_CONSECUTIVE_FAILURES {
            let (next, actions) = transition(machine, Event::SpawnFailed);
            machine = next;
            backoffs.extend(actions.iter().filter_map(|a| match a {
                Action::ScheduleBackoff(d) => Some(*d),
                _ => None,
            }));
            if machine.state == State::Degraded {
                let (next, _) = transition(machine, Event::BackoffElapsed);
                let (next, _) = transition(next, Event::ProbeRefused);
                machine = next;
            }
        }
        assert_eq!(
            machine.state,
            State::Failed,
            "must give up, not loop forever"
        );
        assert_eq!(
            backoffs,
            vec![
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
            ],
            "backoff should grow, and the fifth failure is terminal"
        );
    }

    #[test]
    fn a_failed_kernel_only_restarts_when_the_user_asks() {
        let failed = Machine {
            state: State::Failed,
            owned: false,
            consecutive_failures: MAX_CONSECUTIVE_FAILURES,
        };
        let (unchanged, actions) = transition(failed, Event::BackoffElapsed);
        assert_eq!(unchanged.state, State::Failed);
        assert!(actions.is_empty());

        let (restarted, actions) = transition(failed, Event::UserRequestedStart);
        assert_eq!(restarted.state, State::Probing);
        assert_eq!(
            restarted.consecutive_failures, 0,
            "the user gets a clean slate"
        );
        assert_eq!(actions, vec![Action::Probe]);
    }

    #[test]
    fn recovery_resets_the_failure_count() {
        let degraded = Machine {
            state: State::Degraded,
            owned: false,
            consecutive_failures: 3,
        };
        let (machine, _) = transition(degraded, Event::ProbeFoundOwnKernel);
        assert_eq!(machine.state, State::Ready);
        assert_eq!(machine.consecutive_failures, 0);
    }

    #[test]
    fn quarantine_is_reported_rather_than_retried() {
        // A quarantined binary dies with SIGKILL every single time; retrying it five times just
        // delays a message the user needs immediately.
        let (machine, actions) = drive(
            Machine::default(),
            &[
                Event::Start,
                Event::ProbeRefused,
                Event::SpawnKilledBySignal,
            ],
        );
        assert_eq!(machine.state, State::Failed);
        assert_eq!(actions.last(), Some(&Action::ReportQuarantine));
    }

    #[test]
    fn quitting_the_app_stops_only_our_own_kernel() {
        let ours = Machine {
            state: State::Ready,
            owned: true,
            consecutive_failures: 0,
        };
        let (_, actions) = transition(ours, Event::AppExiting);
        assert_eq!(actions, vec![Action::Terminate]);

        let theirs = Machine {
            state: State::Attached,
            owned: false,
            consecutive_failures: 0,
        };
        let (machine, actions) = transition(theirs, Event::AppExiting);
        assert!(actions.is_empty(), "their server keeps running");
        assert_eq!(machine.state, State::Idle);
    }
}
