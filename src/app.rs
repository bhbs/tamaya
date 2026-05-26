use crate::config::{Config, ensure_local_dir};
use crate::lock::{LockFile, app_lock_name, volume_lock_name};
use crate::registry::Registry;
use anyhow::{Context, Result};
use std::env;

pub fn init() -> Result<()> {
    let root = env::current_dir().context("failed to determine current directory")?;

    ensure_local_dir(&root)?;

    let config = Config::default_for(&root);
    config.create_dirs()?;
    config.save(&root)?;

    let registry = Registry::load(&config.registry_file)?;
    registry.save(&config.registry_file)?;

    println!(
        "initialized {}",
        root.join(crate::config::LOCAL_DIR).display()
    );

    Ok(())
}

pub fn ps() -> Result<()> {
    let config = load_config()?;
    let registry = Registry::load(&config.registry_file)?;

    if registry.apps.is_empty() {
        println!("no apps");
        return Ok(());
    }

    for (name, app) in registry.apps {
        println!("{name}\t{:?}\t{}", app.status, app.port);
    }

    Ok(())
}

pub fn deploy(app: &str) -> Result<()> {
    let config = load_config()?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;
    let _volume_lock = LockFile::acquire(&config.locks_dir, &volume_lock_name(app))?;

    println!("deploy: deploying {app} is not implemented yet");

    Ok(())
}

pub fn rollback(app: &str) -> Result<()> {
    let config = load_config()?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;

    println!("rollback: rolling back {app} is not implemented yet");

    Ok(())
}

pub fn stop(app: &str) -> Result<()> {
    let config = load_config()?;
    let _app_lock = LockFile::acquire(&config.locks_dir, &app_lock_name(app))?;

    println!("stop: stopping {app} is not implemented yet");

    Ok(())
}

pub fn logs(app: &str) -> Result<()> {
    let _config = load_config()?;

    println!("logs: showing logs for {app} is not implemented yet");

    Ok(())
}

fn load_config() -> Result<Config> {
    let root = env::current_dir().context("failed to determine current directory")?;
    Config::load(&root)
}
