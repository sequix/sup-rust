use std::fmt::Display;
use std::ops::DerefMut;
use std::process;
use std::process::Stdio;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use crate::config;
use crate::rotate;
use anyhow::format_err;
use anyhow::Context;
use anyhow::Result;
use log::info;
use nix::sys::stat;
use nix::unistd;

pub struct Process {
    pid: Option<u32>,
    conf: config::Process,
    rotater: Arc<Mutex<rotate::Rotater>>,
    action_mutex: Mutex<()>,
    action_sender: mpsc::Sender<ProcessAction>,
    action_receiver: mpsc::Receiver<ProcessAction>,
}

impl Process {
    pub fn new(conf: config::Process, rotater: rotate::Rotater) -> Result<Self> {
        let pid = None;
        let rotater = Arc::new(Mutex::new(rotater));
        let action_mutex = Mutex::new(());
        let (action_sender, action_receiver) = mpsc::channel();

        let mut p = Process {
            pid,
            conf,
            rotater,
            action_mutex,
            action_sender,
            action_receiver,
        };

        if p.conf.auto_start {
            p.pid = Some(p.new_child().context("failed to start process")?);
        }

        Ok(p)
    }

    fn new_child(&mut self) -> Result<u32> {
        let tmp_path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let fifo_path = Arc::new(tmp_path.to_path_buf());
        tmp_path.close()?;

        unistd::mkfifo(fifo_path.as_path(), stat::Mode::S_IRWXU)
            .context("failed to create log fifo")?;
        info!("created fifo for log at {:?}", fifo_path);

        let rotater = Arc::clone(&self.rotater);
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

        let mut child = process::Command::new(&self.conf.path)
            .args(&self.conf.args)
            .stdout(Stdio::from(log_stdout))
            .stderr(Stdio::from(log_stderr))
            .spawn()
            .context("failed to spawn child process")?;

        info!("spawned child process");
        std::fs::remove_file(fifo_path.as_path()).context("failed to remove log fifo")?;

        thread::sleep(std::time::Duration::from_secs(self.conf.start_seconds));

        let pid = child.id();
        let stat = ProcessStatus::get(pid)?;

        match stat {
            ProcessStatus::Running | ProcessStatus::Sleeping | ProcessStatus::Waiting => {}
            ProcessStatus::None => return Err(format_err!("process exited very quickly")),
            _ => return Err(format_err!("process in state: {stat}")),
        }

        let sender_for_exited = self.action_sender.clone();

        thread::spawn(move || {
            let e = child.wait().unwrap();
            sender_for_exited
                .send(ProcessAction::ProcessExited(e))
                .expect("faield to send process exited action");
        });

        Ok(pid)
    }

    fn waiter(&mut self) -> ! {
        loop {
            let action = self
                .action_receiver
                .recv()
                .expect("failed to receive process action");
            match action {
                ProcessAction::UserAction(action) => match action {
                    config::Action::Start => todo!(),
                    _ => todo!(),
                },
                ProcessAction::ProcessExited(e) => {}
            }
        }
    }

    pub fn start(&mut self) -> Result<()> {
        let _mu = self.action_mutex.lock().unwrap();
        todo!();
    }

    pub fn stop(&mut self) -> Result<()> {
        let _mu = self.action_mutex.lock().unwrap();
        todo!();
    }

    pub fn reload(&mut self) -> Result<()> {
        let _mu = self.action_mutex.lock().unwrap();
        todo!();
    }

    pub fn kill(&mut self) -> Result<()> {
        let _mu = self.action_mutex.lock().unwrap();
        todo!();
    }

    pub fn status(&self) -> Result<String> {
        let _mu = self.action_mutex.lock().unwrap();
        todo!();
    }
}

// /proc/[pid]/stat in https://man7.org/linux/man-pages/man5/proc.5.html
#[derive(Debug)]
enum ProcessStatus {
    None,     // process not existing
    Running,  // R
    Sleeping, // S
    Waiting,  // D
    Zombie,   // Z
    Stopped,  // T
    Tracing,  // t
    Dead,     // X or x
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
            "R" => Ok(ProcessStatus::Running),
            "S" => Ok(ProcessStatus::Sleeping),
            "D" => Ok(ProcessStatus::Waiting),
            "Z" => Ok(ProcessStatus::Zombie),
            "T" => Ok(ProcessStatus::Stopped),
            "t" => Ok(ProcessStatus::Tracing),
            "X" | "x" => Ok(ProcessStatus::Dead),
            _ => Ok(ProcessStatus::Unknown(String::from(stat))),
        }
    }
}

impl Display for ProcessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessStatus::None => write!(f, "NotStarted"),
            ProcessStatus::Running => write!(f, "Running"),
            ProcessStatus::Sleeping => write!(f, "Sleeping"),
            ProcessStatus::Waiting => write!(f, "Waiting"),
            ProcessStatus::Zombie => write!(f, "Zombe"),
            ProcessStatus::Stopped => write!(f, "Stopped"),
            ProcessStatus::Tracing => write!(f, "Tracing"),
            ProcessStatus::Dead => write!(f, "Dead"),
            ProcessStatus::Unknown(s) => write!(f, "Unknown({s})"),
        }
    }
}

#[derive(Debug)]
enum ProcessAction {
    UserAction(config::Action),
    ProcessExited(process::ExitStatus),
}

impl Display for ProcessAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessAction::UserAction(t) => t.fmt(f),
            ProcessAction::ProcessExited(e) => write!(f, "exited({e})"),
        }
    }
}
