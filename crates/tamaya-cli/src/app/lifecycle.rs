use super::{app_name, context, runner};
use anyhow::{Result, bail};

fn with_app(
    app: Option<String>,
    action: impl FnOnce(&crate::ssh::SshRunner, &str) -> Result<()>,
) -> Result<()> {
    let project = context()?;
    let app = app_name(app.as_deref(), project.as_ref())?;
    let (_, ssh) = runner(project.as_ref(), None)?;
    crate::log::step(format!("connecting for {app}"));
    action(&ssh, &app)
}

pub fn rollback(app: Option<String>) -> Result<()> {
    with_app(app, |ssh, app| {
        let output = ssh.rollback(app)?;
        crate::log::result_ready();
        print!("{output}");
        Ok(())
    })
}

pub fn stop(app: Option<String>) -> Result<()> {
    with_app(app, |ssh, app| ssh.stop(app))
}

pub fn delete(app: Option<String>, purge: bool) -> Result<()> {
    with_app(app, |ssh, app| ssh.delete(app, purge))
}

pub fn logs(app: Option<String>) -> Result<()> {
    with_app(app, |ssh, app| ssh.logs(app))
}

pub fn maintenance(
    app: Option<String>,
    domain: Option<String>,
    message: Option<String>,
) -> Result<()> {
    if let Some(domain) = domain {
        if app.is_some() {
            bail!("maintenance accepts either an app or --domain, not both");
        }
        let project = context()?;
        let (_, ssh) = runner(project.as_ref(), None)?;
        return ssh.maintenance_domain(
            &domain,
            message
                .as_deref()
                .unwrap_or("Service temporarily unavailable"),
        );
    }
    with_app(app, |ssh, app| {
        ssh.maintenance(
            app,
            message
                .as_deref()
                .unwrap_or("Service temporarily unavailable"),
        )
    })
}

pub fn live(app: Option<String>, domain: Option<String>) -> Result<()> {
    if let Some(domain) = domain {
        if app.is_some() {
            bail!("live accepts either an app or --domain, not both");
        }
        let project = context()?;
        let (_, ssh) = runner(project.as_ref(), None)?;
        return ssh.live_domain(&domain);
    }
    with_app(app, |ssh, app| ssh.live(app))
}
