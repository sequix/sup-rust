use crate::config::{self, Config};
use anyhow::{Context, Result};
use flate2::{write::GzEncoder, Compression};
use log::{error, info};
use std::{
    ffi::OsStr,
    fs::File,
    io::BufReader,
    path::Path,
    sync::{Arc, Mutex},
    thread,
};

pub struct Rotater {
    conf: config::Log,
    file: File,
    size: u64,
    write_mutex: Arc<Mutex<()>>,
    background_mutex: Arc<Mutex<()>>,
}

impl Rotater {
    pub fn new() -> Result<Self> {
        // TODO: 能否不 clone，同时减少 lifetime quailifier 的使用？
        let conf = Config::get().program.log.clone();
        let file = Self::new_file(&conf.path)?;
        let size = file.metadata().unwrap().len();
        let write_mutex = Arc::new(Mutex::new(()));
        let background_mutex = Arc::new(Mutex::new(()));

        Ok(Rotater {
            conf,
            file,
            size,
            write_mutex,
            background_mutex,
        })
    }

    fn rotate(&mut self) -> Result<()> {
        let dir = Path::new(&self.conf.path)
            .parent()
            .unwrap_or(Path::new("/"));
        let filename = Self::rotated_filename(&self.conf.path);
        let rotated_path = String::from(dir.join(&filename).to_str().unwrap());

        std::fs::rename(&self.conf.path, &rotated_path)
            .context("failed to rename log file to rotated filename")?;

        self.file = Self::new_file(&self.conf.path)?;
        info!("rotated log {} to {rotated_path}", self.conf.path);

        let mu = Arc::clone(&self.background_mutex);
        let compress = self.conf.compress;
        let merge_compressed = self.conf.merge_compressed;

        thread::spawn(move || {
            Self::rotate_background(mu, rotated_path, compress, merge_compressed);
        });

        Ok(())
    }

    fn rotate_background<P: AsRef<Path>>(
        mu: Arc<Mutex<()>>,
        path: P,
        compress: bool,
        merge_compressed: bool,
    ) {
        let _x = mu.lock().unwrap();
        if compress {
            if let Err(e) = Self::gzip(path) {
                error!("failed to gzip rotated log: {e}");
            }
            if merge_compressed {
                todo!();
            }
        }
        if let Err(e) = Self::clean_extra_backups() {
            error!("failed to clean extra backups: {e}");
        }
    }

    fn gzip<P: AsRef<Path>>(path: P) -> Result<()> {
        let path = path.as_ref();
        let file_input = File::open(path).context("failed to open rotated log to gzip")?;
        let mut input = BufReader::new(file_input);

        let path_output = format!("{}.gz", path.as_os_str().to_str().unwrap());
        let file_output = File::create(&path_output)
            .context("failed to open output file for gzipping rotated log")?;
        let mut output = GzEncoder::new(file_output, Compression::default());

        std::io::copy(&mut input, &mut output).context("failed to gzip rotated log")?;

        output
            .finish()
            .context("failed to finish gzipping rotated log")?;

        info!(
            "compressed log {} to {path_output}",
            path.as_os_str().to_str().unwrap()
        );

        Ok(())
    }

    fn clean_extra_backups() -> Result<()> {
        todo!();
    }

    fn new_file<P: AsRef<Path>>(path: P) -> Result<File> {
        let dir = path.as_ref().parent().unwrap_or(Path::new("/"));
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
        format!("{stem}-{}.{ext}", now.format("%Y%m%d%H%M%S"))
    }
}

impl std::io::Write for Rotater {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mutex = Arc::clone(&self.write_mutex);
        let _x = mutex.lock().unwrap();

        let written = self.file.write(buf)?;

        self.size += written as u64;

        if self.conf.max_size > 0 && self.size > self.conf.max_size * 1024 * 1024 {
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
