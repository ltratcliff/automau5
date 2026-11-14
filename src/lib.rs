//! Core automation engine shared by the `am` CLI and the desktop app.
//!
//! The engine is split in two so that front-ends can control *where* input is
//! injected (macOS requires some of it to happen on the main thread):
//!
//! * [`Actor`] performs a single mouse/keyboard action.
//! * [`run_job`] handles the timing loop, cancellation and progress events, and
//!   calls back into the front-end to perform each action. For
//!   [`Action::KeepAwake`] it holds an OS power assertion instead and never
//!   calls back.

mod awake;
mod keys;

pub use awake::AwakeGuard;
pub use keys::KeyCombo;
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use std::fmt;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// Maximum distance (in pixels, per axis) the cursor is nudged by a `Move`.
const MOVE_RANGE: i32 = 250;

/// Default for [`JobConfig::hold_ms`]: long enough for games that poll input
/// once per frame to notice the press.
pub const DEFAULT_HOLD_MS: u64 = 100;
/// Shortest allowed interval. Faster than this, Move/Click can make the
/// computer hard to take back control of.
pub const MIN_INTERVAL_MS: u64 = 50;
const MAX_HOLD_MS: u64 = 10 * 60 * 1000;

/// The kind of input to generate on every iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum Action {
    /// Nudge the cursor by a random offset.
    Move,
    /// Click the left mouse button where the cursor currently is.
    Click,
    /// Press a random lowercase letter, or one of [`JobConfig::keys`].
    Type,
    /// Stop the system going to sleep (and optionally the display) using the
    /// OS power-management API. Generates no input and needs no permissions.
    KeepAwake,
}

impl Action {
    /// Whether this action injects mouse/keyboard input (and so needs an
    /// [`Actor`] and, on macOS, Accessibility permission).
    pub fn needs_input_access(self) -> bool {
        self != Action::KeepAwake
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Action::Move => "move",
            Action::Click => "click",
            Action::Type => "type",
            Action::KeepAwake => "keep-awake",
        })
    }
}

/// What to do, how often, and how many times.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct JobConfig {
    pub action: Action,
    /// Milliseconds to wait before each action (at least [`MIN_INTERVAL_MS`]).
    /// For `KeepAwake` this is just how often progress is reported.
    pub interval_ms: u64,
    /// Number of actions to perform, or `None` to run until stopped.
    pub count: Option<u32>,
    /// `KeepAwake` only: also keep the display from turning off.
    #[cfg_attr(feature = "serde", serde(default))]
    pub keep_display_on: bool,
    /// `Type` only: key combos to press instead of a random letter. With more
    /// than one, each iteration picks one at random.
    #[cfg_attr(feature = "serde", serde(default))]
    pub keys: Vec<KeyCombo>,
    /// `Type` with `keys` only: how long to hold the combo down.
    #[cfg_attr(feature = "serde", serde(default = "default_hold_ms"))]
    pub hold_ms: u64,
}

#[cfg(feature = "serde")]
fn default_hold_ms() -> u64 {
    DEFAULT_HOLD_MS
}

impl JobConfig {
    pub fn validate(&self) -> Result<(), Error> {
        if self.interval_ms < MIN_INTERVAL_MS {
            return Err(Error::new(format!("interval must be at least {MIN_INTERVAL_MS} ms")));
        }
        if self.count == Some(0) {
            return Err(Error::new("count must be at least 1"));
        }
        if !self.keys.is_empty() && self.action != Action::Type {
            return Err(Error::new("keys can only be used with the type action"));
        }
        if self.hold_ms > MAX_HOLD_MS {
            return Err(Error::new("hold must be at most 10 minutes"));
        }
        Ok(())
    }

    /// How long the job runs, or `None` if it runs until stopped.
    pub fn total_duration(&self) -> Option<Duration> {
        self.count
            .map(|count| Duration::from_millis(self.interval_ms.saturating_mul(u64::from(count))))
    }
}

/// Progress notifications emitted by [`run_job`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")
)]
pub enum Event {
    Started {
        action: Action,
        interval_ms: u64,
        count: Option<u32>,
    },
    Performed {
        iteration: u32,
        count: Option<u32>,
        detail: String,
    },
    Failed {
        iteration: u32,
        count: Option<u32>,
        error: String,
    },
    /// All iterations ran.
    Finished { completed: u32 },
    /// The job was cancelled before all iterations ran.
    Stopped { completed: u32 },
}

impl Event {
    /// Whether this is the last event a job will emit.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Event::Finished { .. } | Event::Stopped { .. })
    }
}

#[derive(Debug, Clone)]
pub struct Error(String);

impl Error {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<enigo::NewConError> for Error {
    fn from(e: enigo::NewConError) -> Self {
        Self(format!("could not access input devices: {e}"))
    }
}

impl From<enigo::InputError> for Error {
    fn from(e: enigo::InputError) -> Self {
        Self(format!("input failed: {e}"))
    }
}

/// One step of input for a front-end to carry out via [`Actor::apply`].
///
/// Holding a combo is split into `Press` and `Release` so the wait in between
/// happens in [`run_job`], not on whichever thread performs the input.
#[derive(Debug, Clone)]
pub enum Input {
    Action(Action),
    Press(KeyCombo),
    Release(KeyCombo),
}

/// Performs individual input actions.
///
/// Not `Send`: create and use it on a single thread. On macOS that should be
/// the main thread.
pub struct Actor {
    enigo: Enigo,
    rng: fastrand::Rng,
}

impl Actor {
    /// Connects to the OS input system. On macOS this fails (and shows the
    /// system prompt) until the app is granted Accessibility permission.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            enigo: Enigo::new(&Settings::default())?,
            rng: fastrand::Rng::new(),
        })
    }

    /// Carries out one input step and returns a short description.
    pub fn apply(&mut self, input: &Input) -> Result<String, Error> {
        match input {
            Input::Action(action) => self.perform(*action),
            Input::Press(combo) => {
                for &key in combo.keys() {
                    self.enigo.key(key, Direction::Press)?;
                }
                Ok(format!("Pressed {combo}"))
            }
            Input::Release(combo) => {
                // Release everything even if one key fails, so nothing sticks.
                let mut result = Ok(());
                for &key in combo.keys().iter().rev() {
                    if let Err(e) = self.enigo.key(key, Direction::Release) {
                        result = result.and(Err(e));
                    }
                }
                result?;
                Ok(format!("Released {combo}"))
            }
        }
    }

    /// Performs one action and returns a short human-readable description.
    pub fn perform(&mut self, action: Action) -> Result<String, Error> {
        match action {
            Action::Move => {
                let x = self.rng.i32(-MOVE_RANGE..=MOVE_RANGE);
                let y = self.rng.i32(-MOVE_RANGE..=MOVE_RANGE);
                self.enigo.move_mouse(x, y, Coordinate::Rel)?;
                Ok(format!("Moved cursor by ({x}, {y})"))
            }
            Action::Click => {
                self.enigo.button(Button::Left, Direction::Click)?;
                Ok("Clicked left button".to_string())
            }
            Action::Type => {
                let c = self.rng.lowercase();
                self.enigo.key(Key::Unicode(c), Direction::Click)?;
                Ok(format!("Typed '{c}'"))
            }
            Action::KeepAwake => Ok(AWAKE_DETAIL.to_string()),
        }
    }
}

/// The `detail` reported on each progress tick of a `KeepAwake` job.
pub const AWAKE_DETAIL: &str = "Keeping awake";

/// Runs a job to completion or until `stop` receives a message (or its sender
/// is dropped). Blocks the calling thread for the whole job.
///
/// Waits one interval *before* each action, so the first action does not land
/// on whatever was just used to start the job. A failed action is reported via
/// [`Event::Failed`] and the job carries on.
///
/// `KeepAwake` jobs hold an [`AwakeGuard`] for their whole duration and never
/// call `perform`. `Type` jobs with [`JobConfig::keys`] call it twice per
/// iteration, to press and then release a combo; if stopped while holding,
/// the combo is released before the job ends.
///
/// Returns `Err` without emitting any events if the job can't start;
/// otherwise returns the terminal event, which is also passed to `on_event`.
pub fn run_job(
    config: &JobConfig,
    stop: &Receiver<()>,
    mut perform: impl FnMut(Input) -> Result<String, Error>,
    mut on_event: impl FnMut(Event),
) -> Result<Event, Error> {
    config.validate()?;
    let _awake = match config.action {
        Action::KeepAwake => Some(AwakeGuard::new(config.keep_display_on)?),
        _ => None,
    };

    on_event(Event::Started {
        action: config.action,
        interval_ms: config.interval_ms,
        count: config.count,
    });

    let interval = Duration::from_millis(config.interval_ms);
    let hold = Duration::from_millis(config.hold_ms);
    let mut rng = fastrand::Rng::new();
    let mut completed: u32 = 0;
    while config.count.is_none_or(|count| completed < count) {
        match stop.recv_timeout(interval) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                let event = Event::Stopped { completed };
                on_event(event.clone());
                return Ok(event);
            }
        }

        completed = completed.saturating_add(1);
        let mut stopped = false;
        let result = match config.action {
            Action::KeepAwake => Ok(AWAKE_DETAIL.to_string()),
            Action::Type if !config.keys.is_empty() => {
                let combo = &config.keys[rng.usize(..config.keys.len())];
                let pressed = perform(Input::Press(combo.clone()));
                if pressed.is_ok() && !hold.is_zero() {
                    stopped = !matches!(stop.recv_timeout(hold), Err(RecvTimeoutError::Timeout));
                }
                // Release even after a failed press: some keys may be down.
                let released = perform(Input::Release(combo.clone()));
                pressed.and(released).map(|_| match config.hold_ms {
                    0 => format!("Pressed {combo}"),
                    ms => format!("Pressed {combo} for {ms} ms"),
                })
            }
            action => perform(Input::Action(action)),
        };
        on_event(match result {
            Ok(detail) => Event::Performed {
                iteration: completed,
                count: config.count,
                detail,
            },
            Err(e) => Event::Failed {
                iteration: completed,
                count: config.count,
                error: e.to_string(),
            },
        });
        if stopped {
            let event = Event::Stopped { completed };
            on_event(event.clone());
            return Ok(event);
        }
    }

    let event = Event::Finished { completed };
    on_event(event.clone());
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn config(count: u32) -> JobConfig {
        JobConfig {
            action: Action::Click,
            interval_ms: 1000,
            count: Some(count),
            keep_display_on: false,
            keys: Vec::new(),
            hold_ms: DEFAULT_HOLD_MS,
        }
    }

    fn keys_config(keys: &[&str], hold_ms: u64, count: u32) -> JobConfig {
        JobConfig {
            action: Action::Type,
            keys: keys.iter().map(|k| k.parse().unwrap()).collect(),
            hold_ms,
            ..config(count)
        }
    }

    /// Records the inputs a job asks for, as strings like "press w".
    fn recorder(log: &mut Vec<String>) -> impl FnMut(Input) -> Result<String, Error> + '_ {
        move |input| {
            log.push(match &input {
                Input::Action(a) => format!("action {a}"),
                Input::Press(c) => format!("press {c}"),
                Input::Release(c) => format!("release {c}"),
            });
            Ok(String::new())
        }
    }

    #[test]
    fn keys_are_pressed_then_released() {
        let (_tx, rx) = mpsc::channel();
        let mut log = Vec::new();
        let mut details = Vec::new();
        let cfg = keys_config(&["ctrl+shift+a"], 0, 2);
        run_job(&cfg, &rx, recorder(&mut log), |e| {
            if let Event::Performed { detail, .. } = e {
                details.push(detail);
            }
        })
        .unwrap();
        assert_eq!(log, ["press ctrl+shift+a", "release ctrl+shift+a"].repeat(2));
        assert_eq!(details, ["Pressed ctrl+shift+a", "Pressed ctrl+shift+a"]);
    }

    #[test]
    fn multiple_combos_are_picked_from() {
        let (_tx, rx) = mpsc::channel();
        let mut log = Vec::new();
        let cfg = keys_config(&["w", "a"], 0, 3);
        run_job(&cfg, &rx, recorder(&mut log), |_| {}).unwrap();
        assert!(log.iter().all(|l| ["press w", "release w", "press a", "release a"].contains(&l.as_str())));
        assert_eq!(log.len(), 6);
    }

    #[test]
    fn stop_while_holding_releases_and_stops() {
        let (tx, rx) = mpsc::channel();
        let mut log = Vec::new();
        // Hold far longer than the test waits; the stop arrives mid-hold.
        let cfg = keys_config(&["space"], 60_000, 5);
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1300));
            tx.send(()).unwrap();
        });
        let started = std::time::Instant::now();
        let end = run_job(&cfg, &rx, recorder(&mut log), |_| {}).unwrap();
        stopper.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(log, ["press space", "release space"]);
        assert!(matches!(end, Event::Stopped { completed: 1 }));
    }

    #[test]
    fn keys_require_type_action() {
        let cfg = JobConfig { action: Action::Click, ..keys_config(&["w"], 0, 1) };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn stop_before_first_action() {
        let (tx, rx) = mpsc::channel();
        tx.send(()).unwrap();
        let mut performed = 0;
        let end = run_job(&config(5), &rx, |_| { performed += 1; Ok(String::new()) }, |_| {}).unwrap();
        assert!(matches!(end, Event::Stopped { completed: 0 }));
        assert_eq!(performed, 0);
    }

    #[test]
    fn dropped_sender_stops() {
        let (tx, rx) = mpsc::channel::<()>();
        drop(tx);
        let end = run_job(&config(5), &rx, |_| Ok(String::new()), |_| {}).unwrap();
        assert!(matches!(end, Event::Stopped { completed: 0 }));
    }

    #[test]
    fn failures_are_reported_and_job_continues() {
        let (_tx, rx) = mpsc::channel();
        let mut events = Vec::new();
        let end = run_job(&config(2), &rx, |_| Err(Error::new("nope")), |e| events.push(e)).unwrap();
        assert!(matches!(end, Event::Finished { completed: 2 }));
        let failures = events.iter().filter(|e| matches!(e, Event::Failed { .. })).count();
        assert_eq!(failures, 2);
    }

    #[test]
    fn invalid_config_errors_without_events() {
        let (_tx, rx) = mpsc::channel();
        let mut events = 0;
        assert!(run_job(&config(0), &rx, |_| Ok(String::new()), |_| events += 1).is_err());
        assert_eq!(events, 0);
    }

    #[test]
    fn until_stopped_runs_past_any_count() {
        // Stop it from another thread after a few iterations.
        let (tx, rx) = mpsc::channel();
        let cfg = JobConfig { count: None, ..config(1) };
        let mut performed = 0;
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(2500));
            tx.send(()).unwrap();
        });
        let end = run_job(&cfg, &rx, |_| { performed += 1; Ok(String::new()) }, |_| {}).unwrap();
        handle.join().unwrap();
        assert!(matches!(end, Event::Stopped { completed: 2 }));
        assert_eq!(performed, 2);
    }

    #[test]
    fn keep_awake_never_calls_perform() {
        let (tx, rx) = mpsc::channel();
        tx.send(()).unwrap(); // stop at the first wait
        let cfg = JobConfig { action: Action::KeepAwake, ..config(3) };
        // Taking the power assertion can legitimately fail in a headless test
        // environment (e.g. no D-Bus session on Linux); only check the flow.
        if let Ok(end) = run_job(&cfg, &rx, |_| panic!("perform called"), |_| {}) {
            assert!(matches!(end, Event::Stopped { completed: 0 }));
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn config_json_shape() {
        let cfg: JobConfig =
            serde_json::from_str(r#"{"action":"keepAwake","intervalMs":60000,"count":null}"#).unwrap();
        assert_eq!(cfg.action, Action::KeepAwake);
        assert_eq!(cfg.count, None);
        assert!(!cfg.keep_display_on);
    }

    #[test]
    fn validation() {
        assert!(config(0).validate().is_err());
        assert!(JobConfig { interval_ms: 0, ..config(1) }.validate().is_err());
        assert!(JobConfig { interval_ms: MIN_INTERVAL_MS - 1, ..config(1) }.validate().is_err());
        assert!(JobConfig { interval_ms: MIN_INTERVAL_MS, ..config(1) }.validate().is_ok());
        assert!(config(1).validate().is_ok());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn event_json_shape() {
        let json = serde_json::to_value(Event::Performed {
            iteration: 1,
            count: Some(2),
            detail: "x".into(),
        })
        .unwrap();
        assert_eq!(json["kind"], "performed");
        assert_eq!(json["iteration"], 1);
    }
}
