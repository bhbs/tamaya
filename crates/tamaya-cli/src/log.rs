use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::io::IsTerminal;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[derive(Default)]
struct State {
    command: String,
    progress: Option<Progress>,
}

struct Progress {
    bar: ProgressBar,
    active: bool,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}

pub fn init() {
    let _ = state();
}

pub fn set_command(command: &str) {
    state().lock().unwrap().command = command.to_owned();
}

pub fn log_start() {
    let mut locked = state().lock().unwrap();
    let message = format!("running tamaya {}", locked.command);
    eprintln!("→ {message}");

    let bar = ProgressBar::hidden();
    bar.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    bar.set_message(message);
    locked.progress = Some(Progress {
        bar: bar.clone(),
        active: true,
    });
    drop(locked);

    if std::io::stderr().is_terminal() {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(1));
            let locked = state().lock().unwrap();
            if locked
                .progress
                .as_ref()
                .is_some_and(|progress| progress.active)
            {
                bar.set_draw_target(ProgressDrawTarget::stderr());
                bar.enable_steady_tick(Duration::from_millis(80));
            }
        });
    }
}

pub fn step(message: impl Into<String>) {
    let message = message.into();
    let state = state().lock().unwrap();
    if let Some(progress) = &state.progress {
        progress.bar.set_message(message.clone());
    }
    if !std::io::stderr().is_terminal() {
        eprintln!("  → {message}");
    }
}

pub fn stop_spinner() {
    let mut state = state().lock().unwrap();
    if let Some(progress) = &mut state.progress {
        progress.active = false;
        progress.bar.finish_and_clear();
    }
}

pub fn result_ready() {
    stop_spinner();
}

pub fn log_finish(success: bool) {
    let mut state = state().lock().unwrap();
    let Some(mut progress) = state.progress.take() else {
        return;
    };
    progress.active = false;
    progress.bar.finish_and_clear();
    if success {
        eprintln!("✓ tamaya {} completed", state.command);
    } else {
        eprintln!("✗ tamaya {} failed", state.command);
    }
}
