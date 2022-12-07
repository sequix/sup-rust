use std::fmt::Display;
use std::ops::DerefMut;
use std::process;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use crate::config;
use crate::config::Config;
use crate::rotate;
use anyhow::format_err;
use anyhow::Context;
use anyhow::Result;
use log::info;
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::sys::stat;
use nix::unistd;
use nix::unistd::Pid;

pub struct Process {
    inner: Arc<ProcessInner>,
}

struct ProcessInner {
    conf: config::Process,
    rotater: Arc<Mutex<rotate::Rotater>>,
    id_status: Arc<Mutex<ProcessIdStatus>>,
}

struct ProcessIdStatus {
    pid: Option<u32>,
    desired_status: ProcessStatus,
}

impl Process {
    pub fn new(rotater: rotate::Rotater) -> Result<Self> {
        let conf = Config::get().program.process.clone();
        let rotater = Arc::new(Mutex::new(rotater));

        let id_status = Arc::new(Mutex::new(ProcessIdStatus {
            pid: None,
            desired_status: ProcessStatus::None,
        }));

        let inner = Arc::new(ProcessInner {
            conf,
            rotater,
            id_status,
        });

        let p = Process { inner };

        if p.inner.conf.auto_start {
            let pid = p.call_new_child()?;
            let mut is = p.inner.id_status.lock().unwrap();
            is.pid = Some(pid);
            is.desired_status = ProcessStatus::Running;
        }

        Ok(p)
    }

    fn call_new_child(&self) -> Result<u32> {
        let inner = Arc::clone(&self.inner);
        Self::new_child(inner)
    }

    fn new_child(inner: Arc<ProcessInner>) -> Result<u32> {
        let tmp_path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let fifo_path = Arc::new(tmp_path.to_path_buf());
        tmp_path.close()?;

        unistd::mkfifo(fifo_path.as_path(), stat::Mode::S_IRWXU)
            .context("failed to create log fifo")?;

        let rotater = Arc::clone(&inner.rotater);
        let fifo_path_redirect = fifo_path.clone();

        thread::spawn(move || {
            let mut f =
                std::fs::File::open(fifo_path_redirect.as_path()).expect("failed to open log fifo");
            let mut thread_rotater = rotater.lock().unwrap();
            std::io::copy(&mut f, thread_rotater.deref_mut())
                .expect("failed to copy log from child process to rotataer");
        });

        let log_stdout = std::fs::OpenOptions::new()
            .write(true)
            .open(fifo_path.as_path())
            .context("failed to open fifo for stdout redirecting")?;

        let log_stderr = log_stdout
            .try_clone()
            .context("failed to oen fifo for stderr redirecting")?;

        // TODO: env variables & work dir
        let child = process::Command::new(&inner.conf.path)
            .args(&inner.conf.args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_stdout))
            .stderr(Stdio::from(log_stderr))
            .spawn()
            .context("failed to spawn child process")?;

        info!("spawned child process");
        std::fs::remove_file(fifo_path.as_path()).context("failed to remove log fifo")?;

        thread::sleep(std::time::Duration::from_secs(inner.conf.start_seconds));
        let pid = child.id();
        let stat = ProcessStatus::get(pid)?;

        match stat {
            ProcessStatus::Running => {}
            ProcessStatus::None => return Err(format_err!("process exited very quickly")),
            // TODO: zombie
            _ => return Err(format_err!("process in state: {}", stat)),
        }

        let inner = Arc::clone(&inner);
        thread::spawn(move || Self::child_waiter(inner, child));

        Ok(pid)
    }

    fn child_waiter(inner: Arc<ProcessInner>, mut child: process::Child) {
        let es = child.wait().unwrap();
        info!("child process exited with code {}", es.code().unwrap());

        let mut is = inner.id_status.lock().unwrap();
        is.pid.take();

        if matches!(is.desired_status, ProcessStatus::None) {
            return;
        }

        match inner.conf.restart_strategy {
            config::RestartStrategy::None => {}
            config::RestartStrategy::Always => {
                let inner = Arc::clone(&inner);
                is.pid = Some(Self::new_child(inner).expect("failed to restart process"));
            }
            config::RestartStrategy::OnFailure => {
                if !es.success() {
                    let inner = Arc::clone(&inner);
                    is.pid = Some(Self::new_child(inner).expect("failed to restart process"));
                }
            }
        }
    }

    pub fn start(&mut self) -> Result<()> {
        let mut is = self.inner.id_status.lock().unwrap();
        is.desired_status = ProcessStatus::Running;

        if is.pid.is_none() {
            is.pid = Some(self.call_new_child()?);
        }
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        let mut is = self.inner.id_status.lock().unwrap();
        is.desired_status = ProcessStatus::None;

        if is.pid.is_some() {
            let pid = is.pid.take().unwrap();
            signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
                .context("failed to send SIGTERM to child process")?;
            Self::wait_for_none(pid)?;
        }
        Ok(())
    }

    pub fn reload(&mut self) -> Result<()> {
        let is = self.inner.id_status.lock().unwrap();

        if is.pid.is_some() {
            let pid = is.pid.unwrap();
            signal::kill(Pid::from_raw(pid as i32), Signal::SIGHUP)
                .context("failed to send SIGHUP to child process")?;
        }
        Ok(())
    }

    pub fn kill(&mut self) -> Result<()> {
        let mut is = self.inner.id_status.lock().unwrap();
        is.desired_status = ProcessStatus::None;

        if is.pid.is_some() {
            let pid = is.pid.take().unwrap();
            signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL)
                .context("failed to send SIGKILL to child process")?;
            Self::wait_for_none(pid)?;
        }
        Ok(())
    }

    fn wait_for_none(pid: u32) -> Result<()> {
        loop {
            let stat = ProcessStatus::get(pid)?;
            if matches!(stat, ProcessStatus::None) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Ok(())
    }

    pub fn status(&self) -> Result<String> {
        let is = self.inner.id_status.lock().unwrap();
        let pid = is.pid;

        if pid.is_none() {
            Ok(String::from("NotStarted"))
        } else {
            let status = ProcessStatus::get(pid.unwrap())?;
            Ok(format!("{status}"))
        }
    }
}

// /proc/[pid]/stat in https://man7.org/linux/man-pages/man5/proc.5.html
#[derive(Debug)]
enum ProcessStatus {
    None,    // process not existing
    Running, // R | S | D
    Zombie,  // Z
    Unknown(String),
}

impl ProcessStatus {
    fn get(pid: u32) -> Result<Self> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"));

        if let Err(e) = stat {
            match e.kind() {
                std::io::ErrorKind::NotFound => return Ok(ProcessStatus::None),
                _ => return Err(format_err!("failed to read /proc/{pid}/stat: {e}")),
            }
        }
        let stat = stat.unwrap();
        let stat = stat.split_whitespace().skip(2).next().unwrap();

        match stat {
            "R" | "S" | "D" => Ok(ProcessStatus::Running),
            "Z" => Ok(ProcessStatus::Zombie),
            _ => Ok(ProcessStatus::Unknown(String::from(stat))),
        }
    }
}

impl Display for ProcessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessStatus::None => write!(f, "NotStarted"),
            ProcessStatus::Running => write!(f, "Running"),
            ProcessStatus::Zombie => write!(f, "Zombie"),
            ProcessStatus::Unknown(s) => write!(f, "Unknown({s})"),
        }
    }
}

/*
// TODO: unit test

// mantaining:
1.process exit quicker than start_seconds
2.process exit later than start_seconds, and diff restart_strategy
3.process env & workdir

// action:
1.auto_start false & start action
2.running & stop action & start action & stop action

// log:
1.redirect both stdout & stderr
2.logrotate file & compress
3.delete extra logs & merge gzips
*/
