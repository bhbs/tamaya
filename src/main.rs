use std::env;
use std::process;

fn main() {
    let mut args = env::args().skip(1);

    let result = match args.next().as_deref() {
        None | Some("-h") | Some("--help") => {
            print_help();
            Ok(())
        }
        Some("init") => init(),
        Some("ps") => ps(),
        Some("deploy") => deploy(args.collect()),
        Some("rollback") => rollback(args.collect()),
        Some("stop") => stop(args.collect()),
        Some("logs") => logs(args.collect()),
        Some(command) => Err(format!("unknown command: {command}")),
    };

    if let Err(error) = result {
        eprintln!("error: {error}");
        eprintln!();
        print_help();
        process::exit(1);
    }
}

fn print_help() {
    println!(
        "\
v - lightweight Firecracker PaaS control CLI

Usage:
  v <command> [args]

Commands:
  init                Initialize host directories and config
  deploy <app>        Deploy an immutable app image
  rollback <app>      Roll back an app to the previous image
  ps                  List managed microVMs
  stop <app>          Stop an app microVM
  logs <app>          Show app logs
"
    );
}

fn init() -> Result<(), String> {
    println!("init: host initialization is not implemented yet");
    Ok(())
}

fn ps() -> Result<(), String> {
    println!("ps: VM listing is not implemented yet");
    Ok(())
}

fn deploy(args: Vec<String>) -> Result<(), String> {
    let app = required_app("deploy", &args)?;
    println!("deploy: deploying {app} is not implemented yet");
    Ok(())
}

fn rollback(args: Vec<String>) -> Result<(), String> {
    let app = required_app("rollback", &args)?;
    println!("rollback: rolling back {app} is not implemented yet");
    Ok(())
}

fn stop(args: Vec<String>) -> Result<(), String> {
    let app = required_app("stop", &args)?;
    println!("stop: stopping {app} is not implemented yet");
    Ok(())
}

fn logs(args: Vec<String>) -> Result<(), String> {
    let app = required_app("logs", &args)?;
    println!("logs: showing logs for {app} is not implemented yet");
    Ok(())
}

fn required_app(command: &str, args: &[String]) -> Result<String, String> {
    match args {
        [app] => Ok(app.clone()),
        [] => Err(format!("{command} requires an app name")),
        _ => Err(format!("{command} accepts exactly one app name")),
    }
}
