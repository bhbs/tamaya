use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

fn fmt_time() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() % 86400;
    let h = secs / 3600;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

pub fn init() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let level = record.level();
            let target = record.target();
            let level_style = buf.default_level_style(level);
            let target_style = env_logger::fmt::style::AnsiColor::Cyan.on_default();

            writeln!(
                buf,
                " {} {level_style}{level}{level_style:#} [{target_style}{target}{target_style:#}] {}",
                fmt_time(),
                record.args()
            )
        })
        .target(env_logger::Target::Stdout)
        .init();
}
