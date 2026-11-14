use automau5::{run_job, Action, Actor, Event, JobConfig, KeyCombo, DEFAULT_HOLD_MS};
use clap::Parser;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Instant;

const ENDOFAUTO: &str = r#"

############################################
Automated mouse typing and clicking complete
Thank you for using this program!
############################################
"#;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// What to do on each iteration
    #[arg(value_enum)]
    action: Action,

    /// Number of seconds between moving or clicking
    #[arg(short, long, default_value_t = 60, conflicts_with = "interval_ms")]
    interval: u64,

    /// Interval in milliseconds instead of seconds (minimum 50)
    #[arg(long, value_name = "MS")]
    interval_ms: Option<u64>,

    /// Number of iterations to run program
    #[arg(short, long, default_value_t = 60, conflicts_with = "forever")]
    count: u32,

    /// Run until interrupted with Ctrl-C instead of a fixed count
    #[arg(short, long)]
    forever: bool,

    /// keep-awake only: also keep the display from turning off
    #[arg(short, long)]
    display: bool,

    /// type only: press this key or combo instead of a random letter, e.g.
    /// `space`, `f5`, `ctrl+shift+a`. Repeat to pick one at random each time.
    #[arg(short, long = "key", value_name = "COMBO")]
    keys: Vec<KeyCombo>,

    /// type --key only: milliseconds to hold the combo down
    #[arg(long, value_name = "MS", default_value_t = DEFAULT_HOLD_MS)]
    hold: u64,
}

/// "250 ms", "1.5s", "60s"
fn format_interval(ms: u64) -> String {
    match ms {
        ms if ms < 1000 => format!("{ms} ms"),
        ms if ms % 1000 == 0 => format!("{}s", ms / 1000),
        ms => format!("{:.1}s", ms as f64 / 1000.0),
    }
}

/// Prints `[h:mm:ss] message`, timed from the start of the run.
fn log(start: Instant, msg: impl std::fmt::Display) {
    let s = start.elapsed().as_secs();
    println!("[{}:{:02}:{:02}] {msg}", s / 3600, s / 60 % 60, s % 60);
}

fn main() -> ExitCode {
    let args = Args::parse();
    let start = Instant::now();

    let config = JobConfig {
        action: args.action,
        interval_ms: args.interval_ms.unwrap_or(args.interval.saturating_mul(1000)),
        count: (!args.forever).then_some(args.count),
        keep_display_on: args.display,
        keys: args.keys,
        hold_ms: args.hold,
    };
    if let Err(e) = config.validate() {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    // keep-awake doesn't touch input devices, so don't ask for access.
    let mut actor = if config.action.needs_input_access() {
        match Actor::new() {
            Ok(actor) => Some(actor),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };

    match config.total_duration() {
        Some(total) => log(
            start,
            format_args!(
                "Running '{}' every {}, {} times ({:.2} hours)",
                config.action,
                format_interval(config.interval_ms),
                config.count.unwrap_or_default(),
                total.as_secs_f32() / 3600.
            ),
        ),
        None => log(
            start,
            format_args!(
                "Running '{}' every {} until Ctrl-C",
                config.action,
                format_interval(config.interval_ms)
            ),
        ),
    }

    // Never signalled: the CLI runs until done or Ctrl-C.
    let (_stop_tx, stop_rx) = mpsc::channel();
    let result = run_job(
        &config,
        &stop_rx,
        |input| match actor.as_mut() {
            Some(actor) => actor.apply(&input),
            None => unreachable!("run_job only calls perform for input actions"),
        },
        |event| {
            let (iteration, count, msg) = match &event {
                Event::Performed { iteration, count, detail } => (iteration, count, detail),
                Event::Failed { iteration, count, error } => (iteration, count, error),
                _ => return,
            };
            let progress = match count {
                Some(count) => format!("{iteration}/{count}"),
                None => iteration.to_string(),
            };
            if matches!(event, Event::Failed { .. }) {
                eprintln!("error: [{progress}] {msg}");
            } else {
                log(start, format_args!("[{progress}] {msg}"));
            }
        },
    );
    if let Err(e) = result {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    println!("{ENDOFAUTO}");
    ExitCode::SUCCESS
}
