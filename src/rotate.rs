use crate::config;
use anyhow::{Context, Result};
use log::error;
use std::{
    ffi::OsStr,
    fs::File,
    path::Path,
    sync::{Arc, Mutex},
};

pub struct Rotater {
    conf: config::Log,
    file: File,
    size: u64,
    write_mutex: Arc<Mutex<()>>,
}

impl Rotater {
    pub fn new(conf: config::Log) -> Result<Rotater> {
        let file = Self::new_file(&conf.path)?;
        let size = file.metadata().unwrap().len();
        let write_mutex = Arc::new(Mutex::new(()));

        Ok(Rotater {
            conf,
            file,
            size,
            write_mutex,
        })
    }

    fn rotate(&mut self) -> Result<()> {
        let dir = Path::new(&self.conf.path)
            .parent()
            .unwrap_or(Path::new("/"));
        let filename = Self::rotated_filename(&self.conf.path);
        let rotated_path = dir.join(&filename);

        std::fs::rename(&self.conf.path, rotated_path)
            .context("failed to rename log file to rotated filename")?;

        self.file = Self::new_file(&self.conf.path)?;

        Ok(())
    }

    fn new_file(path: &str) -> Result<File> {
        let dir = Path::new(path).parent().unwrap();
        if !dir.exists() {
            std::fs::create_dir_all(dir).context("failed to create parent directory for log")?;
        }
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .append(true)
            .open(path)
            .context("failed to open log file for rotater")?;
        Ok(file)
    }

    fn rotated_filename(path: &str) -> String {
        let now = chrono::Utc::now();
        let path = Path::new(path);
        let ext = path.extension().and_then(OsStr::to_str).unwrap_or_default();
        let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
        format!("{stem}-{}{ext}", now.format("%Y%m%d%H%M%S"))
    }
}

impl std::io::Write for Rotater {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mutex = self.write_mutex.clone();
        let _x = mutex.lock().unwrap();

        let written = self.file.write(buf)?;

        self.size += written as u64;

        if self.conf.max_size > 0 && self.size > self.conf.max_size {
            if let Err(e) = self.rotate() {
                error!("failed to rotate log {e}");
            }
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
