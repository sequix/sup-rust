use std::{
    io::{BufRead, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
};

use crate::config;
use anyhow::{format_err, Context, Result};
use log::{error, info};

pub fn run(socket: &str) -> Result<()> {
    run_singal_handler()?;
    run_server(socket)?;
    Ok(())
}

fn run_singal_handler() -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();

    ctrlc::set_handler(move || tx.send(()).expect("cannot send signal on channel"))
        .context("failed to set singal handler")?;

    std::thread::spawn(move || {
        rx.recv().expect("failed to receive from singal channel");
        handle_singal();
    });

    Ok(())
}

fn handle_singal() {
    todo!();
}

fn run_server(socket_path: &str) -> Result<()> {
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
                std::thread::spawn(|| {
                    if let Err(e) = handle_client(c) {
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

fn handle_client(mut c: UnixStream) -> Result<()> {
    use config::Action;

    let mut buf = [0; 64];
    let len = c.read(&mut buf)?;

    let action = std::str::from_utf8(&buf[..len])?;

    info!("received action {action}");

    match Action::from(action) {
        Action::Start => {}
        Action::Stop => {}
        Action::Restart => {}
        Action::Reload => {}
        Action::Kill => {}
        Action::Status => {}
        Action::Exit => {}
        _ => {
            let rsp = format!("do not support action {action}");
            c.write_all(rsp.as_bytes())
                .context("failed to write error message back")?;
        }
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
