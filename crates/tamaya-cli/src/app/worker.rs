use super::{context, runner};
use anyhow::Result;

pub fn setup(worker: Option<String>) -> Result<()> {
    let project = context()?;
    let (name, ssh) = runner(project.as_ref(), worker.as_deref())?;
    crate::log::step(format!("setting up worker {name}"));
    ssh.setup()?;
    crate::log::result_ready();
    println!("worker {name} ready");
    Ok(())
}

pub fn check(worker: Option<String>) -> Result<()> {
    let project = context()?;
    let (name, ssh) = runner(project.as_ref(), worker.as_deref())?;
    crate::log::step(format!("checking worker {name}"));
    let result = ssh.check()?;
    crate::log::result_ready();
    print!("{}", result.output);
    println!("worker: {name}");
    if !result.success {
        anyhow::bail!("worker {name} is not ready");
    }
    Ok(())
}
