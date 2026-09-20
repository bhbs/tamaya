use crate::{
    app::app_name,
    config::ProjectConfig,
    ssh::{SshRunner, validate_name},
};
use anyhow::{Context, Result, bail};
use std::io::Read;

pub fn set(app: Option<&str>, key: &str, stdin: bool) -> Result<()> {
    let (app, ssh) = context(app)?;
    validate_name("app", &app)?;
    validate_key(key)?;
    let value = read_value(key, stdin)?;
    validate_value(&value)?;

    crate::log::step(format!("updating environment variables for {app}"));
    ssh.set_env(&app, key, value.as_bytes())?;
    crate::log::result_ready();
    println!("set {key}");
    Ok(())
}

pub fn unset(app: Option<&str>, key: &str) -> Result<()> {
    let (app, ssh) = context(app)?;
    validate_name("app", &app)?;
    // Older versions accepted names that systemd ignores. Keep those names
    // removable while requiring valid systemd names for newly set values.
    validate_name("environment variable key", key)?;

    crate::log::step(format!("updating environment variables for {app}"));
    ssh.unset_env(&app, key)?;
    crate::log::result_ready();
    println!("unset {key}");
    Ok(())
}

pub fn list(app: Option<&str>) -> Result<()> {
    let (app, ssh) = context(app)?;
    validate_name("app", &app)?;

    crate::log::step(format!("loading environment variables for {app}"));
    let keys = ssh.list_env(&app)?;
    crate::log::result_ready();
    if keys.trim().is_empty() {
        println!("(no environment variables set for {app:?})");
    } else {
        print!("{keys}");
    }
    Ok(())
}

fn context(app: Option<&str>) -> Result<(String, SshRunner)> {
    let project = ProjectConfig::load()?;
    let app = app_name(app, project.as_ref())?;
    let (_, worker) = crate::config::worker_with_project(None, project.as_ref())?;
    Ok((app, SshRunner::new(worker)))
}

pub(crate) fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() {
        bail!("environment variable key must not be empty");
    }
    if !key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        bail!(
            "environment variable key must start with an ASCII letter or '_' and contain only ASCII letters, digits or '_': {key:?}"
        );
    }
    Ok(())
}

fn validate_value(value: &str) -> Result<()> {
    if value.contains(['\n', '\r']) {
        bail!("environment variable value must not contain newlines");
    }
    if value.chars().any(|c| {
        let code = c as u32;
        c == '\0' || c == '\u{feff}' || (0xfdd0..=0xfdef).contains(&code) || code & 0xffff >= 0xfffe
    }) {
        bail!("environment variable value contains a character unsupported by systemd");
    }
    Ok(())
}

pub(crate) fn encode_value(value: &[u8]) -> Result<String> {
    let value =
        std::str::from_utf8(value).context("environment variable value must be valid UTF-8")?;
    validate_value(value)?;
    // EnvironmentFile uses shell-like double quotes, not shell evaluation.
    // Match systemd's escaping so whitespace and literal backslashes survive.
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    for c in value.chars() {
        if matches!(c, '\\' | '"' | '$' | '`') {
            encoded.push('\\');
        }
        encoded.push(c);
    }
    encoded.push('"');
    Ok(encoded)
}

fn read_value(key: &str, stdin: bool) -> Result<String> {
    if !stdin {
        return rpassword::prompt_password(format!("{key}: "))
            .context("failed to read environment variable value");
    }
    let mut value = String::new();
    std::io::stdin()
        .read_to_string(&mut value)
        .context("failed to read environment variable value from stdin")?;
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_keys() {
        assert!(validate_key("DATABASE_URL").is_ok());
        assert!(validate_key("_API_KEY2").is_ok());
        assert!(validate_key("API-KEY2").is_err());
        assert!(validate_key("2TOKEN").is_err());
        assert!(validate_key("").is_err());
        assert!(validate_key("BAD KEY").is_err());
        assert!(validate_key("BAD=value").is_err());
    }

    #[test]
    fn rejects_newlines_in_value() {
        assert!(validate_value("one line").is_ok());
        assert!(validate_value("line one\nline two").is_err());
        assert!(validate_value("line one\rline two").is_err());
    }

    #[test]
    fn rejects_values_systemd_cannot_load() {
        for value in [
            "nul\0byte",
            "\u{feff}",
            "\u{fdd0}",
            "\u{fdef}",
            "\u{fffe}",
            "\u{10ffff}",
        ] {
            assert!(encode_value(value.as_bytes()).is_err());
        }
        assert!(encode_value(&[0xff]).is_err());
        assert!(encode_value("\t日本語🙂\u{fdf0}".as_bytes()).is_ok());
    }

    #[test]
    fn encodes_environment_file_values_without_changing_literals() {
        for (value, expected) in [
            ("", "\"\""),
            ("  padded\t ", "\"  padded\t \""),
            ("a'b#;=日本語🙂", "\"a'b#;=日本語🙂\""),
            (
                r#""quoted" $HOME `cmd` \n C:\path\"#,
                r#""\"quoted\" \$HOME \`cmd\` \\n C:\\path\\""#,
            ),
        ] {
            assert_eq!(encode_value(value.as_bytes()).unwrap(), expected);
        }
    }
}
