//! Reconciling the dial with the daemon's effective setpoint.
//!
//! The daemon is the brain: Alexa, the web app and the schedule all change the
//! effective setpoint, and the dial should follow. But the user's own pending
//! change wins until it has been published and echoed back (or timed out).

use std::time::{Duration, Instant};

use pish_core::DaemonState;

/// How long to wait for the daemon to echo a setpoint we published before
/// letting the daemon's value through anyway.
pub(crate) const ECHO_TIMEOUT: Duration = Duration::from_secs(15);

/// Tolerance when comparing setpoints (the dial moves in 0.5 steps).
const EPSILON: f32 = 0.05;

/// The setpoint the dial should show for this state, if it has one.
pub(crate) fn state_setpoint(state: &DaemonState) -> Option<f32> {
    state.setpoint.filter(|s| s.is_finite())
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StateSync {
    /// Highest state `seq` already dealt with.
    applied_seq: u64,
    /// A setpoint we published, awaiting the daemon's echo, and its deadline.
    awaiting: Option<(f32, Instant)>,
}

impl StateSync {
    /// Record that the dial's value was just published.
    pub(crate) fn on_publish(&mut self, value: f32, now: Instant) {
        self.awaiting = Some((value, now + ECHO_TIMEOUT));
    }

    /// `latest` is the newest `(seq, effective setpoint)` from the daemon;
    /// `busy` is true while the user is touching the dial or has an
    /// unpublished change. Returns `Some(setpoint)` when the dial (and the
    /// debouncer's baseline) should move to it.
    pub(crate) fn reconcile(
        &mut self,
        latest: Option<(u64, f32)>,
        busy: bool,
        now: Instant,
    ) -> Option<f32> {
        let (seq, setpoint) = latest?;
        if seq <= self.applied_seq || busy {
            return None;
        }
        if let Some((value, deadline)) = self.awaiting {
            if now >= deadline {
                self.awaiting = None;
            } else if (setpoint - value).abs() < EPSILON {
                // Echo of our own change: the dial is already right.
                self.awaiting = None;
                self.applied_seq = seq;
                return None;
            } else {
                // Stale relative to our change: hold off, revisit at the deadline.
                return None;
            }
        }
        self.applied_seq = seq;
        Some(setpoint)
    }

    /// Time until a pending echo wait expires, for scheduling a repaint.
    pub(crate) fn time_to_timeout(&self, now: Instant) -> Option<Duration> {
        self.awaiting
            .map(|(_, deadline)| deadline.saturating_duration_since(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn first_state_moves_the_dial() {
        let mut s = StateSync::default();
        assert_eq!(
            s.reconcile(Some((1, 21.5)), false, Instant::now()),
            Some(21.5)
        );
    }

    #[test]
    fn same_seq_is_applied_once() {
        let mut s = StateSync::default();
        let t = Instant::now();
        assert_eq!(s.reconcile(Some((1, 21.0)), false, t), Some(21.0));
        assert_eq!(s.reconcile(Some((1, 21.0)), false, t + ms(100)), None);
        assert_eq!(s.reconcile(Some((2, 22.0)), false, t + ms(200)), Some(22.0));
    }

    #[test]
    fn no_state_does_nothing() {
        assert_eq!(
            StateSync::default().reconcile(None, false, Instant::now()),
            None
        );
    }

    #[test]
    fn busy_defers_without_consuming() {
        let mut s = StateSync::default();
        let t = Instant::now();
        assert_eq!(s.reconcile(Some((1, 21.0)), true, t), None);
        // Once the user is done, the deferred state is applied.
        assert_eq!(s.reconcile(Some((1, 21.0)), false, t + ms(10)), Some(21.0));
    }

    #[test]
    fn published_change_wins_until_echoed() {
        let mut s = StateSync::default();
        let t = Instant::now();
        s.on_publish(23.0, t);
        // An old state (pre-change) arrives: ignore.
        assert_eq!(s.reconcile(Some((5, 20.0)), false, t + ms(500)), None);
        assert!(s.time_to_timeout(t + ms(500)).is_some());
        // The echo arrives: dial already right, nothing to apply, wait ends.
        assert_eq!(s.reconcile(Some((6, 23.0)), false, t + ms(900)), None);
        assert_eq!(s.time_to_timeout(t + ms(900)), None);
        // Later external change flows through.
        assert_eq!(
            s.reconcile(Some((7, 19.0)), false, t + ms(2000)),
            Some(19.0)
        );
    }

    #[test]
    fn echo_within_tolerance_counts() {
        let mut s = StateSync::default();
        let t = Instant::now();
        s.on_publish(21.5, t);
        assert_eq!(s.reconcile(Some((1, 21.5001)), false, t), None);
        assert_eq!(s.time_to_timeout(t), None);
    }

    #[test]
    fn timeout_lets_the_daemon_value_through() {
        let mut s = StateSync::default();
        let t = Instant::now();
        s.on_publish(23.0, t);
        assert_eq!(s.reconcile(Some((1, 20.0)), false, t + ms(1000)), None);
        assert_eq!(
            s.reconcile(Some((1, 20.0)), false, t + ECHO_TIMEOUT),
            Some(20.0),
            "daemon never echoed: it wins"
        );
        assert_eq!(s.time_to_timeout(t + ECHO_TIMEOUT), None);
    }

    #[test]
    fn busy_during_echo_wait_still_defers() {
        let mut s = StateSync::default();
        let t = Instant::now();
        s.on_publish(23.0, t);
        assert_eq!(s.reconcile(Some((1, 23.0)), true, t + ms(10)), None);
        assert_eq!(s.reconcile(Some((1, 23.0)), false, t + ms(20)), None);
        assert_eq!(s.time_to_timeout(t + ms(20)), None, "echo consumed");
    }

    #[test]
    fn timeout_countdown_saturates() {
        let mut s = StateSync::default();
        let t = Instant::now();
        s.on_publish(21.0, t);
        assert_eq!(s.time_to_timeout(t), Some(ECHO_TIMEOUT));
        assert_eq!(
            s.time_to_timeout(t + ECHO_TIMEOUT * 2),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn non_finite_setpoint_is_not_used() {
        use pish_core::sensor::{parse_message, Message};
        let Some(Message::State(st)) = parse_message(r#"{"type":"state","setpoint":19.5}"#) else {
            panic!()
        };
        assert_eq!(state_setpoint(&st), Some(19.5));
        let mut st2 = *st;
        st2.setpoint = Some(f32::NAN);
        assert_eq!(state_setpoint(&st2), None);
    }
}
