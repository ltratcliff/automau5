// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use automau5::{run_job, Actor, Error, Event, Input, JobConfig};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Name of the event the frontend listens to for job progress.
const JOB_EVENT: &str = "job-event";
/// Sent after a job starts: `null` if the stop hotkey is active, otherwise
/// why it couldn't be registered.
const HOTKEY_EVENT: &str = "stop-hotkey";
/// The stop hotkey in key-combo text form (see automau5::KeyCombo).
const STOP_HOTKEY_COMBO: &str = "shift+escape";

/// Global hotkey that stops the running job, even when the app isn't focused.
/// Registered only while a job runs, so it never steals Shift+Esc otherwise.
/// A registered hotkey (not a keyboard listener) needs no extra permissions
/// and only ever sees this one combination.
fn stop_hotkey() -> Shortcut {
    Shortcut::new(Some(Modifiers::SHIFT), Code::Escape)
}

thread_local! {
    // Lives on the main thread only: macOS requires keyboard input (and its
    // permission prompt) to happen there, so we do it everywhere for consistency.
    static ACTOR: RefCell<Option<Actor>> = const { RefCell::new(None) };
}

/// Runs `f` with the main-thread actor, creating it on first use. Must be
/// called on the main thread.
fn with_actor<T>(f: impl FnOnce(&mut Actor) -> Result<T, Error>) -> Result<T, Error> {
    ACTOR.with(|cell| {
        let mut slot = cell.borrow_mut();
        let actor = match slot.as_mut() {
            Some(actor) => actor,
            None => slot.insert(Actor::new()?),
        };
        f(actor)
    })
}

/// Called from the worker thread: hops to the main thread for one input step
/// and waits for the result. Key holds are waited out on the worker, between
/// separate press and release steps, so the UI never blocks.
fn perform_on_main_thread(app: &AppHandle, input: Input) -> Result<String, Error> {
    let (tx, rx) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = tx.send(with_actor(|actor| actor.apply(&input)));
    })
    .map_err(|e| Error::new(e.to_string()))?;
    rx.recv()
        .map_err(|_| Error::new("app is shutting down"))?
}

struct RunningJob {
    id: u64,
    config: JobConfig,
    stop: mpsc::Sender<()>,
}

#[derive(Default)]
struct JobState {
    current: Mutex<Option<RunningJob>>,
    next_id: AtomicU64,
}

impl JobState {
    fn request_stop(&self) {
        if let Some(job) = self.current.lock().unwrap().as_ref() {
            let _ = job.stop.send(());
        }
    }

    fn clear_if(&self, id: u64) {
        let mut current = self.current.lock().unwrap();
        if current.as_ref().is_some_and(|job| job.id == id) {
            *current = None;
        }
    }
}

/// Starts a job and returns once it is running, or with the reason it
/// couldn't start.
///
/// Synchronous commands run on the main thread, which is where the actor must
/// be created, so input permission problems are reported right here.
#[tauri::command]
fn start_job(app: AppHandle, state: State<'_, JobState>, config: JobConfig) -> Result<(), String> {
    config.validate().map_err(|e| e.to_string())?;
    if config.keys.iter().any(|combo| combo.to_string() == STOP_HOTKEY_COMBO) {
        return Err("Shift+Esc is the stop hotkey, so it can't be one of the keys to press".into());
    }
    if config.action.needs_input_access() {
        with_actor(|_| Ok(())).map_err(|e| e.to_string())?;
    }

    let mut current = state.current.lock().unwrap();
    if current.is_some() {
        return Err("A job is already running".into());
    }
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let (stop_tx, stop_rx) = mpsc::channel();
    *current = Some(RunningJob {
        id,
        config: config.clone(),
        stop: stop_tx,
    });
    drop(current);

    // The worker reports here once the job has started (or failed to). It
    // never needs the main thread before then, so blocking on it is safe.
    let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);

    // The job gets its own long-lived thread: keep-awake's power assertion
    // is per-thread on Windows and is held for the whole job.
    std::thread::spawn(move || {
        let mut ready = Some(ready_tx);
        let result = run_job(
            &config,
            &stop_rx,
            |input| perform_on_main_thread(&app, input),
            |event| {
                let started = matches!(event, Event::Started { .. });
                if started {
                    if let Some(tx) = ready.take() {
                        let _ = tx.send(Ok(()));
                    }
                }
                if event.is_terminal() {
                    // Release the hotkey before clearing state, so a new job
                    // can't register it first and then lose it to this unregister.
                    let _ = app.global_shortcut().unregister(stop_hotkey());
                    // Clear state before announcing the end, so the UI can
                    // start a new job as soon as it hears about it.
                    app.state::<JobState>().clear_if(id);
                }
                let _ = app.emit(JOB_EVENT, event);
                if started {
                    // register() blocks until the main thread handles it, which
                    // is only safe now that start_job has returned.
                    let status = app.global_shortcut().register(stop_hotkey());
                    let _ = app.emit(HOTKEY_EVENT, status.err().map(|e| e.to_string()));
                }
            },
        );
        if let Err(e) = result {
            app.state::<JobState>().clear_if(id);
            if let Some(tx) = ready.take() {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    ready_rx
        .recv()
        .unwrap_or_else(|_| Err("the job stopped unexpectedly".into()))
}

#[tauri::command]
fn stop_job(state: State<'_, JobState>) {
    state.request_stop();
}

/// The running job's config, if any. Lets the UI recover after a reload.
#[tauri::command]
fn job_status(state: State<'_, JobState>) -> Option<JobConfig> {
    state
        .current
        .lock()
        .unwrap()
        .as_ref()
        .map(|job| job.config.clone())
}

/// WebKitGTK on some Wayland compositors (seen on Hyprland with HiDPI scaling)
/// reports a garbage device pixel ratio, which wrecks the layout. Running
/// through XWayland avoids it. (Input injection is unaffected: enigo talks to
/// Wayland directly.) Respects an explicit GDK_BACKEND.
#[cfg(target_os = "linux")]
fn prefer_x11_backend() {
    if std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        // Safe here: called first thing in main, before any threads exist.
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    prefer_x11_backend();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state == ShortcutState::Pressed && shortcut.id() == stop_hotkey().id() {
                        app.state::<JobState>().request_stop();
                    }
                })
                .build(),
        )
        .manage(JobState::default())
        .invoke_handler(tauri::generate_handler![start_job, stop_job, job_status])
        .run(tauri::generate_context!())
        .expect("error while running automau5");
}
