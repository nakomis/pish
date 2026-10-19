//! Live readings from the nakostat sensor daemon.
//!
//! The daemon (nakostat `embedded/pi/sensor-daemon`) runs on the same Pi and
//! writes one JSON-encoded `SensorReading` per line to every client of its Unix
//! socket. This module is a pure read-only client of that stream:
//!
//! - [`SensorSource`] abstracts "open a line stream", so tests can feed canned
//!   lines instead of a real socket.
//! - [`Reconnect`] is the pure reconnect/backoff state machine.
//! - [`SensorFeed`] runs the reader on a background thread and publishes the
//!   latest [`SensorStatus`] through a [`SensorHandle`], which the UI thread
//!   reads without ever blocking on I/O.

use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Where the daemon listens unless configured otherwise.
pub const DEFAULT_SOCKET_PATH: &str = "/run/nakostat/sensor.sock";

/// Environment override, shared with the daemon so one setting moves both ends.
pub const SOCKET_PATH_ENV: &str = "NAKOSTAT_SOCKET_PATH";

// ── Config ────────────────────────────────────────────────────────────────────

/// Sensor socket settings. Deserialisable so it can sit in pish's TOML config;
/// every field has a default, so an absent section is fine.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct SensorConfig {
    pub socket_path: PathBuf,
}

impl Default for SensorConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from(DEFAULT_SOCKET_PATH),
        }
    }
}

impl SensorConfig {
    /// Defaults plus the `NAKOSTAT_SOCKET_PATH` override from the environment.
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        cfg.apply_overrides(|k| std::env::var(k).ok());
        cfg
    }

    /// Apply environment overrides. `get` is injected so tests needn't touch
    /// the process-global environment.
    pub fn apply_overrides(&mut self, get: impl Fn(&str) -> Option<String>) {
        if let Some(v) = get(SOCKET_PATH_ENV).filter(|v| !v.is_empty()) {
            self.socket_path = PathBuf::from(v);
        }
    }
}

// ── Wire format ───────────────────────────────────────────────────────────────

/// Mirror of the daemon's `SensorReading`. Unknown fields are ignored so the
/// daemon can grow its payload without breaking pish.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SensorReading {
    pub temperature_c: f32,
    pub humidity_pct: f32,
    pub eco2_ppm: u16,
    pub tvoc_ppb: u16,
    /// True while the ENS160 is still warming up (~3 min from cold start).
    pub warming_up: bool,
    pub timestamp: DateTime<Utc>,
}

/// Parse one line of the daemon's stream. Blank or malformed lines yield `None`
/// and are simply skipped by the reader.
pub fn parse_line(line: &str) -> Option<SensorReading> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    serde_json::from_str(line).ok()
}

/// The daemon's effective control state: the `pi/state` object, sent as a
/// `{"type":"state",…}` line. Every field but `setpoint` and `mode` is optional
/// and unknown fields are ignored, so the daemon can evolve without breaking pish.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DaemonState {
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub humidity: Option<f32>,
    /// `"heat"` or `"off"`.
    #[serde(default = "default_mode")]
    pub mode: String,
    /// The effective setpoint (override if active, else the schedule).
    #[serde(default)]
    pub setpoint: Option<f32>,
    #[serde(default)]
    pub scheduled_setpoint: Option<f32>,
    #[serde(default, rename = "override")]
    pub override_: Option<Override>,
    #[serde(default)]
    pub demand: bool,
    #[serde(default)]
    pub demand_source: Option<String>,
    #[serde(default)]
    pub heatlink: Option<Heatlink>,
}

fn default_mode() -> String {
    "heat".into()
}

/// A temporary setpoint override and when it lapses.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Override {
    pub setpoint: f32,
    pub until: DateTime<Utc>,
}

/// What the daemon last heard from the boiler controller.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Heatlink {
    /// `"auto"`, `"fallback"`, `"force_on"` or `"force_off"`.
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub relay: Option<bool>,
    #[serde(default)]
    pub switch: Option<String>,
    #[serde(default)]
    pub seen: Option<DateTime<Utc>>,
}

/// A state as received, with when and in which order, so staleness and
/// "is this new?" can be judged without trusting the daemon's clock.
#[derive(Debug, Clone, PartialEq)]
pub struct StateSnapshot {
    pub state: DaemonState,
    pub received_at: Instant,
    /// Increases by one with every state line received (never reset).
    pub seq: u64,
}

/// One line of the daemon's stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Reading(SensorReading),
    State(Box<DaemonState>),
}

/// Parse a line of either kind. Lines with an unknown `type`, lines that
/// match neither shape, and blank lines yield `None`. Reading lines carry no
/// `type` field.
pub fn parse_message(line: &str) -> Option<Message> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    match value.get("type") {
        None => serde_json::from_value(value).ok().map(Message::Reading),
        Some(t) if t == "state" => serde_json::from_value(value)
            .ok()
            .map(|s| Message::State(Box::new(s))),
        Some(_) => None,
    }
}

// ── Reconnect state machine ───────────────────────────────────────────────────

/// Exponential backoff: `initial`, doubling on each consecutive failure, capped
/// at `max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackoffPolicy {
    pub initial: Duration,
    pub max: Duration,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(30),
        }
    }
}

impl BackoffPolicy {
    /// Delay before retry number `attempt` (1-based).
    pub fn delay(&self, attempt: u32) -> Duration {
        let shift = attempt.saturating_sub(1).min(31);
        self.initial.saturating_mul(1u32 << shift).min(self.max)
    }
}

/// Where the connection to the daemon currently stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinkState {
    /// First connection attempt, not yet resolved.
    Connecting,
    /// Socket open; readings flow as the daemon emits them.
    Connected,
    /// The socket was absent, refused, or dropped. Waiting `delay` before
    /// retry number `attempt`.
    Retrying { attempt: u32, delay: Duration },
}

/// Pure reconnect/backoff state machine — no I/O, no clocks.
#[derive(Debug, Clone)]
pub struct Reconnect {
    policy: BackoffPolicy,
    state: LinkState,
    failures: u32,
}

impl Reconnect {
    pub fn new(policy: BackoffPolicy) -> Self {
        Self {
            policy,
            state: LinkState::Connecting,
            failures: 0,
        }
    }

    pub fn state(&self) -> LinkState {
        self.state
    }

    /// A connection succeeded: the backoff starts afresh next time.
    pub fn on_connected(&mut self) {
        self.failures = 0;
        self.state = LinkState::Connected;
    }

    /// A connect failed or an open connection dropped. Returns how long to wait.
    pub fn on_failure(&mut self) -> Duration {
        self.failures = self.failures.saturating_add(1);
        let delay = self.policy.delay(self.failures);
        self.state = LinkState::Retrying {
            attempt: self.failures,
            delay,
        };
        delay
    }
}

impl Default for Reconnect {
    fn default() -> Self {
        Self::new(BackoffPolicy::default())
    }
}

// ── Sources ───────────────────────────────────────────────────────────────────

/// Something that can be (re)opened as a line stream of daemon readings.
pub trait SensorSource: Send + 'static {
    fn connect(&mut self) -> io::Result<Box<dyn BufRead + Send>>;
}

/// The real source: the daemon's Unix domain socket.
#[cfg(unix)]
pub struct UnixSocketSource {
    path: PathBuf,
}

#[cfg(unix)]
impl UnixSocketSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[cfg(unix)]
impl SensorSource for UnixSocketSource {
    fn connect(&mut self) -> io::Result<Box<dyn BufRead + Send>> {
        let stream = std::os::unix::net::UnixStream::connect(&self.path)?;
        Ok(Box::new(BufReader::new(stream)))
    }
}

// ── Shared status ─────────────────────────────────────────────────────────────

/// What the UI needs to know: the link state and the most recent reading
/// received on the current connection.
#[derive(Debug, Clone, PartialEq)]
pub struct SensorStatus {
    pub link: LinkState,
    pub latest: Option<SensorReading>,
    /// The latest daemon state on the current connection, if any.
    pub state: Option<StateSnapshot>,
}

impl Default for SensorStatus {
    fn default() -> Self {
        Self {
            link: LinkState::Connecting,
            latest: None,
            state: None,
        }
    }
}

/// Cheap, cloneable view onto the feed's latest status. Safe to read every
/// frame: the lock is only ever held to copy a small struct, never across I/O.
#[derive(Debug, Clone, Default)]
pub struct SensorHandle {
    shared: Arc<Mutex<SensorStatus>>,
}

impl SensorHandle {
    pub fn status(&self) -> SensorStatus {
        self.shared.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn update(&self, f: impl FnOnce(&mut SensorStatus)) {
        if let Ok(mut s) = self.shared.lock() {
            f(&mut s);
        }
    }
}

// ── Background reader ─────────────────────────────────────────────────────────

/// Owns the background reader thread. Dropping it asks the thread to stop;
/// the thread notices between lines or during a backoff wait.
pub struct SensorFeed {
    handle: SensorHandle,
    stop: Arc<AtomicBool>,
}

impl SensorFeed {
    /// Start reading the daemon socket from `config` with the default backoff.
    #[cfg(unix)]
    pub fn spawn_unix(config: &SensorConfig) -> Self {
        Self::spawn(
            UnixSocketSource::new(&config.socket_path),
            Reconnect::default(),
        )
    }

    pub fn spawn<S: SensorSource>(source: S, reconnect: Reconnect) -> Self {
        let handle = SensorHandle::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (h, s) = (handle.clone(), stop.clone());
        thread::Builder::new()
            .name("pish-sensor".into())
            .spawn(move || run(source, reconnect, &h, &s))
            .expect("spawn sensor reader thread");
        Self { handle, stop }
    }

    pub fn handle(&self) -> SensorHandle {
        self.handle.clone()
    }
}

impl Drop for SensorFeed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run<S: SensorSource>(
    mut source: S,
    mut reconnect: Reconnect,
    handle: &SensorHandle,
    stop: &AtomicBool,
) {
    let mut seq = 0u64;
    while !stop.load(Ordering::Relaxed) {
        if let Ok(reader) = source.connect() {
            reconnect.on_connected();
            handle.update(|s| {
                *s = SensorStatus {
                    link: LinkState::Connected,
                    ..Default::default()
                }
            });
            for line in reader.lines() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(line) = line else { break };
                match parse_message(&line) {
                    Some(Message::Reading(reading)) => {
                        handle.update(|s| s.latest = Some(reading));
                    }
                    Some(Message::State(state)) => {
                        seq += 1;
                        let snapshot = StateSnapshot {
                            state: *state,
                            received_at: Instant::now(),
                            seq,
                        };
                        handle.update(|s| s.state = Some(snapshot));
                    }
                    None => {}
                }
            }
        }
        // Socket absent, refused, or closed by the daemon: back off and retry.
        let delay = reconnect.on_failure();
        let link = reconnect.state();
        handle.update(|s| {
            *s = SensorStatus {
                link,
                ..Default::default()
            }
        });
        sleep_unless_stopped(delay, stop);
    }
}

/// Sleep for `total`, waking early if `stop` is set.
fn sleep_unless_stopped(total: Duration, stop: &AtomicBool) {
    const SLICE: Duration = Duration::from_millis(100);
    let mut left = total;
    while !left.is_zero() && !stop.load(Ordering::Relaxed) {
        let step = left.min(SLICE);
        thread::sleep(step);
        left -= step;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::Cursor;
    use std::time::Instant;

    /// A line exactly as the daemon's `serde_json::to_string(&reading)` emits it.
    const VALID_LINE: &str = r#"{"temperature_c":21.0,"humidity_pct":50.0,"eco2_ppm":600,"tvoc_ppb":100,"warming_up":false,"timestamp":"2026-10-13T09:30:00.123456789Z"}"#;

    fn test_reading() -> SensorReading {
        SensorReading {
            temperature_c: 21.0,
            humidity_pct: 50.0,
            eco2_ppm: 600,
            tvoc_ppb: 100,
            warming_up: false,
            timestamp: "2026-10-13T09:30:00.123456789Z".parse().unwrap(),
        }
    }

    fn fast_policy() -> BackoffPolicy {
        BackoffPolicy {
            initial: Duration::from_millis(5),
            max: Duration::from_millis(20),
        }
    }

    /// Wait (briefly) until `pred` holds for the handle's status.
    fn wait_for(handle: &SensorHandle, pred: impl Fn(&SensorStatus) -> bool) -> SensorStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let s = handle.status();
            if pred(&s) {
                return s;
            }
            assert!(Instant::now() < deadline, "timed out; last status {s:?}");
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// Canned source: each connect pops the next scripted outcome; once the
    /// script runs out, connects fail as though the socket were absent.
    struct CannedSource {
        script: VecDeque<Option<String>>,
    }

    impl CannedSource {
        fn new(script: Vec<Option<&str>>) -> Self {
            Self {
                script: script.into_iter().map(|s| s.map(str::to_owned)).collect(),
            }
        }
    }

    impl SensorSource for CannedSource {
        fn connect(&mut self) -> io::Result<Box<dyn BufRead + Send>> {
            match self.script.pop_front().flatten() {
                Some(text) => Ok(Box::new(Cursor::new(text.into_bytes()))),
                None => Err(io::Error::new(io::ErrorKind::NotFound, "no daemon")),
            }
        }
    }

    /// Never-ending stream: hands out `line` forever, so the connection stays up.
    struct Endless(String);

    impl io::Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            thread::sleep(Duration::from_millis(1));
            let n = self.0.len().min(buf.len());
            buf[..n].copy_from_slice(&self.0.as_bytes()[..n]);
            Ok(n)
        }
    }

    struct EndlessSource;

    impl SensorSource for EndlessSource {
        fn connect(&mut self) -> io::Result<Box<dyn BufRead + Send>> {
            Ok(Box::new(BufReader::new(Endless(format!("{VALID_LINE}\n")))))
        }
    }

    // ── Parsing ──

    #[test]
    fn parses_valid_line() {
        assert_eq!(parse_line(VALID_LINE), Some(test_reading()));
    }

    #[test]
    fn parses_line_with_trailing_newline() {
        assert_eq!(parse_line(&format!("{VALID_LINE}\n")), Some(test_reading()));
    }

    #[test]
    fn parses_warming_up_flag() {
        let line = VALID_LINE.replace("\"warming_up\":false", "\"warming_up\":true");
        assert!(parse_line(&line).unwrap().warming_up);
    }

    #[test]
    fn tolerates_unknown_fields() {
        let line = VALID_LINE.replace('}', r#","pressure_hpa":1013.2,"aqi":2}"#);
        assert_eq!(parse_line(&line), Some(test_reading()));
    }

    #[test]
    fn ignores_malformed_line() {
        assert_eq!(parse_line("{\"temperature_c\": 21.0, \"humid"), None);
        assert_eq!(parse_line("not json at all"), None);
    }

    #[test]
    fn ignores_line_missing_a_field() {
        let line = VALID_LINE.replace("\"tvoc_ppb\":100,", "");
        assert_eq!(parse_line(&line), None);
    }

    #[test]
    fn ignores_bad_timestamp() {
        let line = VALID_LINE.replace("2026-10-13T09:30:00.123456789Z", "yesterday");
        assert_eq!(parse_line(&line), None);
    }

    #[test]
    fn ignores_blank_line() {
        assert_eq!(parse_line(""), None);
        assert_eq!(parse_line("   \n"), None);
    }

    // ── Message parsing ──

    const STATE_LINE: &str = r#"{"type":"state","timestamp":"2026-10-19T08:00:00Z","temperature":19.8,"humidity":45.2,"mode":"heat","setpoint":20.5,"scheduled_setpoint":20.0,"override":{"setpoint":20.5,"until":"2026-10-19T09:30:00Z"},"demand":true,"demand_source":"override","heatlink":{"mode":"auto","relay":true,"switch":"auto","seen":"2026-10-19T08:00:00Z"}}"#;

    #[test]
    fn message_reading_line_without_type() {
        assert_eq!(
            parse_message(VALID_LINE),
            Some(Message::Reading(test_reading()))
        );
    }

    #[test]
    fn message_state_line_full() {
        let Some(Message::State(s)) = parse_message(STATE_LINE) else {
            panic!("expected state");
        };
        assert_eq!(s.mode, "heat");
        assert_eq!(s.setpoint, Some(20.5));
        assert_eq!(s.scheduled_setpoint, Some(20.0));
        assert_eq!(s.override_.as_ref().unwrap().setpoint, 20.5);
        assert!(s.demand);
        assert_eq!(s.demand_source.as_deref(), Some("override"));
        let h = s.heatlink.unwrap();
        assert_eq!(h.mode.as_deref(), Some("auto"));
        assert_eq!(h.relay, Some(true));
        assert_eq!(h.switch.as_deref(), Some("auto"));
        assert!(h.seen.is_some());
    }

    #[test]
    fn message_state_with_nulls_and_missing_fields() {
        let line =
            r#"{"type":"state","mode":"off","setpoint":16.0,"override":null,"heatlink":null}"#;
        let Some(Message::State(s)) = parse_message(line) else {
            panic!("expected state");
        };
        assert_eq!(s.mode, "off");
        assert!(s.override_.is_none() && s.heatlink.is_none());
        assert!(!s.demand);
        assert_eq!(s.timestamp, None);
    }

    #[test]
    fn message_state_mode_defaults_to_heat() {
        let Some(Message::State(s)) = parse_message(r#"{"type":"state","setpoint":19.0}"#) else {
            panic!("expected state");
        };
        assert_eq!(s.mode, "heat");
    }

    #[test]
    fn message_state_ignores_unknown_fields() {
        let line = STATE_LINE.replace('}', r#","future":1}"#);
        assert!(matches!(parse_message(&line), Some(Message::State(_))));
    }

    #[test]
    fn message_unknown_type_is_ignored() {
        assert_eq!(parse_message(r#"{"type":"banana","setpoint":1}"#), None);
        // Even if it otherwise looks like a reading.
        let line = VALID_LINE.replace('{', r#"{"type":"other","#);
        assert_eq!(parse_message(&line), None);
    }

    #[test]
    fn message_garbage_blank_and_bad_state_are_ignored() {
        assert_eq!(parse_message(""), None);
        assert_eq!(parse_message("  \n"), None);
        assert_eq!(parse_message("not json"), None);
        assert_eq!(parse_message("[1,2]"), None);
        assert_eq!(parse_message(r#"{"type":"state","setpoint":"hot"}"#), None);
    }

    #[test]
    fn plain_parse_line_rejects_state_lines() {
        assert_eq!(parse_line(STATE_LINE), None);
    }

    // ── Config ──

    #[test]
    fn config_defaults_to_daemon_path() {
        assert_eq!(
            SensorConfig::default().socket_path,
            PathBuf::from("/run/nakostat/sensor.sock")
        );
    }

    #[test]
    fn config_env_override_matches_daemon_variable() {
        let mut cfg = SensorConfig::default();
        cfg.apply_overrides(|k| (k == "NAKOSTAT_SOCKET_PATH").then(|| "/tmp/test.sock".into()));
        assert_eq!(cfg.socket_path, PathBuf::from("/tmp/test.sock"));
    }

    #[test]
    fn config_empty_env_override_is_ignored() {
        let mut cfg = SensorConfig::default();
        cfg.apply_overrides(|_| Some(String::new()));
        assert_eq!(cfg, SensorConfig::default());
    }

    #[test]
    fn config_from_env_without_override_is_default_or_env() {
        let cfg = SensorConfig::from_env();
        match std::env::var(SOCKET_PATH_ENV) {
            Ok(v) if !v.is_empty() => assert_eq!(cfg.socket_path, PathBuf::from(v)),
            _ => assert_eq!(cfg, SensorConfig::default()),
        }
    }

    #[test]
    fn config_deserialises_with_and_without_path() {
        let cfg: SensorConfig = serde_json::from_str(r#"{"socket_path":"/tmp/x.sock"}"#).unwrap();
        assert_eq!(cfg.socket_path, PathBuf::from("/tmp/x.sock"));
        let cfg: SensorConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg, SensorConfig::default());
    }

    // ── Backoff state machine ──

    #[test]
    fn backoff_schedule_doubles_then_caps() {
        let mut r = Reconnect::default();
        let schedule: Vec<u64> = (0..9).map(|_| r.on_failure().as_millis() as u64).collect();
        assert_eq!(
            schedule,
            [500, 1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000]
        );
    }

    #[test]
    fn backoff_starts_connecting() {
        assert_eq!(Reconnect::default().state(), LinkState::Connecting);
    }

    #[test]
    fn backoff_failure_enters_retrying() {
        let mut r = Reconnect::default();
        r.on_failure();
        r.on_failure();
        assert_eq!(
            r.state(),
            LinkState::Retrying {
                attempt: 2,
                delay: Duration::from_secs(1)
            }
        );
    }

    #[test]
    fn backoff_resets_after_successful_connect() {
        let mut r = Reconnect::default();
        for _ in 0..5 {
            r.on_failure();
        }
        r.on_connected();
        assert_eq!(r.state(), LinkState::Connected);
        // A drop after a good connection starts the schedule again from the bottom.
        assert_eq!(r.on_failure(), Duration::from_millis(500));
    }

    #[test]
    fn backoff_survives_huge_attempt_counts() {
        let p = BackoffPolicy::default();
        assert_eq!(p.delay(u32::MAX), p.max);
        assert_eq!(p.delay(0), p.initial);
    }

    // ── Background feed (canned source) ──

    #[test]
    fn handle_default_is_connecting_with_no_reading() {
        assert_eq!(SensorHandle::default().status(), SensorStatus::default());
    }

    #[test]
    fn feed_reports_no_daemon_when_connect_fails() {
        let feed = SensorFeed::spawn(CannedSource::new(vec![]), Reconnect::new(fast_policy()));
        let s = wait_for(&feed.handle(), |s| {
            matches!(s.link, LinkState::Retrying { .. })
        });
        assert!(s.latest.is_none());
    }

    #[test]
    fn feed_delivers_latest_reading_skipping_malformed_lines() {
        let warm = VALID_LINE.replace("\"warming_up\":false", "\"warming_up\":true");
        let stream = format!("{VALID_LINE}\ngarbage\n{warm}\n{{\"half\":\n");
        // The connection stays up so we can observe the latest reading.
        struct Once(Option<String>);
        impl SensorSource for Once {
            fn connect(&mut self) -> io::Result<Box<dyn BufRead + Send>> {
                let text = self.0.take().ok_or_else(|| io::Error::other("gone"))?;
                // Blank keep-alive lines (ignored) keep the connection open.
                let tail = Endless("\n".into());
                Ok(Box::new(BufReader::new(io::Read::chain(
                    Cursor::new(text),
                    tail,
                ))))
            }
        }
        let feed = SensorFeed::spawn(Once(Some(stream)), Reconnect::new(fast_policy()));
        let s = wait_for(&feed.handle(), |s| {
            s.latest.as_ref().is_some_and(|r| r.warming_up)
        });
        assert_eq!(s.link, LinkState::Connected);
        assert_eq!(s.latest.unwrap().eco2_ppm, 600);
    }

    #[test]
    fn feed_delivers_state_with_increasing_seq() {
        struct Once(Option<String>);
        impl SensorSource for Once {
            fn connect(&mut self) -> io::Result<Box<dyn BufRead + Send>> {
                let text = self.0.take().ok_or_else(|| io::Error::other("gone"))?;
                Ok(Box::new(BufReader::new(io::Read::chain(
                    Cursor::new(text),
                    Endless("\n".into()),
                ))))
            }
        }
        let off = STATE_LINE.replace("\"mode\":\"heat\"", "\"mode\":\"off\"");
        let stream = format!("{STATE_LINE}\n{VALID_LINE}\n{{\"type\":\"x\"}}\n{off}\n");
        let feed = SensorFeed::spawn(Once(Some(stream)), Reconnect::new(fast_policy()));
        let s = wait_for(&feed.handle(), |s| {
            s.state.as_ref().is_some_and(|st| st.seq == 2)
        });
        assert_eq!(s.state.unwrap().state.mode, "off");
        assert_eq!(s.latest, Some(test_reading()));
    }

    #[test]
    fn feed_reconnects_after_drop() {
        use std::sync::atomic::AtomicUsize;

        // Connects, sends one reading, drops; fails twice; reconnects; then the
        // script runs out and every further connect fails.
        struct Counting(CannedSource, Arc<AtomicUsize>);
        impl SensorSource for Counting {
            fn connect(&mut self) -> io::Result<Box<dyn BufRead + Send>> {
                self.1.fetch_add(1, Ordering::Relaxed);
                self.0.connect()
            }
        }
        let connects = Arc::new(AtomicUsize::new(0));
        let script = CannedSource::new(vec![Some(VALID_LINE), None, None, Some(VALID_LINE)]);
        let feed = SensorFeed::spawn(
            Counting(script, connects.clone()),
            Reconnect::new(fast_policy()),
        );

        // Once the backoff has climbed to the cap, the reader has been through
        // the whole script and kept retrying rather than giving up.
        let s = wait_for(&feed.handle(), |s| {
            matches!(s.link, LinkState::Retrying { delay, .. } if delay == Duration::from_millis(20))
                && connects.load(Ordering::Relaxed) >= 6
        });
        assert!(s.latest.is_none());
    }

    #[test]
    fn feed_stays_connected_while_lines_flow() {
        let feed = SensorFeed::spawn(EndlessSource, Reconnect::new(fast_policy()));
        let s = wait_for(&feed.handle(), |s| s.latest.is_some());
        assert_eq!(s.link, LinkState::Connected);
        assert_eq!(s.latest, Some(test_reading()));
    }

    #[test]
    fn dropping_feed_stops_thread_during_backoff() {
        let policy = BackoffPolicy {
            initial: Duration::from_secs(60),
            max: Duration::from_secs(60),
        };
        let feed = SensorFeed::spawn(CannedSource::new(vec![]), Reconnect::new(policy));
        let h = feed.handle();
        wait_for(&h, |s| matches!(s.link, LinkState::Retrying { .. }));
        drop(feed);
        // The thread exits within one sleep slice; the handle stays readable.
        thread::sleep(Duration::from_millis(200));
        assert!(matches!(
            h.status().link,
            LinkState::Retrying { attempt: 1, .. }
        ));
    }

    // ── Real Unix socket ──

    #[cfg(unix)]
    #[test]
    fn unix_source_reads_from_a_real_socket() {
        use std::io::Write;
        use std::os::unix::net::UnixListener;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sensor.sock");

        // Nothing listening yet: the feed must report no daemon.
        let config = SensorConfig {
            socket_path: path.clone(),
        };
        let feed = SensorFeed::spawn(
            UnixSocketSource::new(&config.socket_path),
            Reconnect::new(fast_policy()),
        );
        let h = feed.handle();
        wait_for(&h, |s| matches!(s.link, LinkState::Retrying { .. }));

        // Daemon appears: the feed reconnects and picks up the reading.
        let listener = UnixListener::bind(&path).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        writeln!(stream, "{VALID_LINE}").unwrap();
        let s = wait_for(&h, |s| s.latest.is_some());
        assert_eq!(s.link, LinkState::Connected);

        // Daemon goes away: back to retrying, stale reading cleared.
        drop(stream);
        drop(listener);
        let s = wait_for(&h, |s| matches!(s.link, LinkState::Retrying { .. }));
        assert!(s.latest.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn spawn_unix_uses_configured_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = SensorConfig {
            socket_path: dir.path().join("absent.sock"),
        };
        let feed = SensorFeed::spawn_unix(&config);
        wait_for(&feed.handle(), |s| {
            matches!(s.link, LinkState::Retrying { attempt: 1, .. })
        });
    }
}
