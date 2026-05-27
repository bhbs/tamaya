use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, Read};
use std::time::Duration;

pub fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("spinner template is valid"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub fn upload_bar(size: u64) -> ProgressBar {
    let pb = ProgressBar::new(size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {bytes}/{total_bytes}")
            .expect("upload bar template is valid")
            .progress_chars("##-"),
    );
    pb
}

pub struct ProgressReader<R> {
    inner: R,
    pb: ProgressBar,
}

impl<R> ProgressReader<R> {
    pub fn new(inner: R, pb: ProgressBar) -> Self {
        Self { inner, pb }
    }
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.pb.inc(n as u64);
        }
        Ok(n)
    }
}
