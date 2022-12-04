use std::{
    io::{BufRead, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::{Arc, Mutex},
};

use crate::{config, process};
use anyhow::{format_err, Context, Result};
use log::{error, info};

pub fn run(socket: &str, process: process::Process) -> Result<()> {
    let process = Arc::new(Mutex::new(process));
    run_stop_singal_handler(Arc::clone(&process))?;
    run_server(socket, Arc::clone(&process))?;
    Ok(())
}

fn run_stop_singal_handler(process: Arc<Mutex<process::Process>>) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();

    ctrlc::set_handler(move || tx.send(()).expect("cannot send signal on channel"))
        .context("failed to set singal handler")?;

    std::thread::spawn(move || {
        rx.recv().expect("failed to receive from singal channel");
        handle_stop_singal(process);
    });

    Ok(())
}

fn handle_stop_singal(process: Arc<Mutex<process::Process>>) {
    let mut proc = process.lock().unwrap();
    info!("received stop signal, stopping process...");
    if let Err(e) = proc.stop() {
        error!("failed to stop process {e}");
        std::process::exit(1);
    }
    std::process::exit(0);
}

fn run_server(socket_path: &str, process: Arc<Mutex<process::Process>>) -> Result<()> {
    let socket = std::path::Path::new(socket_path);

    if socket.exists() {
        if is_socket_being_used(socket_path) {
            return Err(format_err!("sup socket is being used by another process"));
        }
        std::fs::remove_file(socket).context("failed to remove old sup socket file")?;
    }

    let server = UnixListener::bind(socket).context("failed to bind sup socket")?;

    for client in server.incoming() {
        match client {
            Ok(c) => {
                let proc = Arc::clone(&process);
                std::thread::spawn(|| {
                    if let Err(e) = handle_client(c, proc) {
                        error!("failed to handle client: {e}");
                    }
                });
            }
            Err(e) => {
                error!("faield to accept: {e}");
            }
        }
    }

    Ok(())
}

fn handle_client(mut c: UnixStream, process: Arc<Mutex<process::Process>>) -> Result<()> {
    use config::Action;

    let mut buf = [0; 64];
    let len = c.read(&mut buf)?;
    let action = std::str::from_utf8(&buf[..len])?;
    info!("received action {action}");

    let mut proc = process.lock().unwrap();

    let rsp = match Action::from(action) {
        Action::Start => proc.start(),
        Action::Stop => proc.stop(),
        Action::Reload => proc.reload(),
        Action::Kill => proc.kill(),
        Action::Exit => proc.stop(),
        Action::Restart => {
            if let Err(e) = proc.stop() {
                Err(e)
            } else {
                proc.start()
            }
        }
        Action::Status => match proc.status() {
            Ok(s) => Err(format_err!("{s}")),
            Err(e) => Err(format_err!("{e}")),
        },
        Action::Serve => Err(format_err!("do not support action {action}")),
        
    };

    let rsp = match rsp {
        Ok(()) => String::from("OK"),
        Err(e) => format!("{e}"),
    };

    c.write_all(rsp.as_bytes())
        .context("failed to write error message back")?;

    if action == Action::Exit.to_string() {
        std::process::exit(0);
    }

    Ok(())
}

fn is_socket_being_used(path: &str) -> bool {
    let f_info = "/proc/net/unix";
    let f = std::fs::File::open(f_info).expect("failed to open {f_info}");

    for line in std::io::BufReader::new(f).lines() {
        if line.unwrap().contains(path) {
            return true;
        }
    }
    false
}
