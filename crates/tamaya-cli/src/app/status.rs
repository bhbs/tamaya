use super::{context, runner};
use anyhow::Result;

pub fn status(app: Option<String>) -> Result<()> {
    let project = context()?;
    let (_, ssh) = runner(project.as_ref(), None)?;
    crate::log::step("loading worker status");
    let output = ssh.status(app.as_deref())?;
    crate::log::result_ready();
    print!("{}", format_status(&output));
    Ok(())
}

fn format_status(output: &str) -> String {
    let mut rows = output
        .lines()
        .map(|line| {
            let mut row = line.split('\t').map(str::to_owned).collect::<Vec<_>>();
            if row.len() == 10 {
                row[8] = format_megabytes(&row[8]);
                row[9] = format_milliseconds(&row[9]);
            } else if row.len() == 9 {
                row[7] = format_megabytes(&row[7]);
                row[8] = format_milliseconds(&row[8]);
            }
            row
        })
        .collect::<Vec<_>>();
    if rows.iter().any(|row| row.len() == 1) {
        return output.to_owned();
    }
    let headers = if rows.iter().any(|row| row.len() == 10) {
        vec![
            "APP",
            "STATE",
            "RELEASE",
            "PORT",
            "DOMAIN",
            "PATH",
            "ROUTE",
            "TYPE",
            "MEMORY_MB",
            "CPU_MS",
        ]
    } else {
        vec![
            "APP",
            "STATE",
            "RELEASE",
            "PORT",
            "DOMAIN",
            "PATH",
            "TYPE",
            "MEMORY_MB",
            "CPU_MS",
        ]
    };
    rows.insert(0, headers.into_iter().map(str::to_owned).collect());

    let mut widths = vec![0; rows.iter().map(Vec::len).max().unwrap_or(0)];
    for row in &rows {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(value.len());
        }
    }

    let mut formatted = String::new();
    for row in rows {
        for (index, value) in row.iter().enumerate() {
            formatted.push_str(value);
            if index + 1 < row.len() {
                formatted.push_str(&" ".repeat(widths[index] - value.len() + 2));
            }
        }
        formatted.push('\n');
    }
    formatted
}

fn format_megabytes(value: &str) -> String {
    value
        .parse::<u64>()
        .map(|bytes| format!("{:.2}", bytes as f64 / 1_000_000.0))
        .unwrap_or_else(|_| value.to_owned())
}

fn format_milliseconds(value: &str) -> String {
    value
        .parse::<u64>()
        .map(|nanoseconds| format!("{:.2}", nanoseconds as f64 / 1_000_000.0))
        .unwrap_or_else(|_| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::format_status;

    #[test]
    fn status_rows_are_aligned_by_column_width() {
        let output = "\
demo-bun\tactive\t20260602152626\t20002\thttp://bun.example\t/\troot\tprocess\t120344576\t3443748000\n\
demo-go\tactive\t20260602152315\t20005\thttp://go.example\t/api\tpath\tprocess\t2826240\t26829000\n\
docs\trunning\t20260602153000\t\texample.com\t/docs\tpath\tpublished/static\t\t\n";
        assert_eq!(
            format_status(output),
            "\
APP       STATE    RELEASE         PORT   DOMAIN              PATH   ROUTE  TYPE              MEMORY_MB  CPU_MS\n\
demo-bun  active   20260602152626  20002  http://bun.example  /      root   process           120.34     3443.75\n\
demo-go   active   20260602152315  20005  http://go.example   /api   path   process           2.83       26.83\n\
docs      running  20260602153000         example.com         /docs  path   published/static             \n"
        );
    }

    #[test]
    fn status_message_is_preserved() {
        assert_eq!(format_status("no apps\n"), "no apps\n");
    }
}
