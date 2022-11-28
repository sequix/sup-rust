use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
};

use crate::config::Action;
use anyhow::{format_err, Context, Result};

pub fn request(socket: &str, action: Action) -> Result<String> {
    if matches!(action, Action::Serve) {
        Err(format_err!(
            "client does not support the action {}",
            Action::Serve
        ))?;
    }

    let mut conn =
        UnixStream::connect(Path::new(socket)).context("failed to connect to sup socket")?;

    let action = action.to_string();

    conn.write_all(action.as_bytes())
        .context("failed to send action")?;

    let mut rsp = String::new();
    conn.read_to_string(&mut rsp)
        .context("failed to receive response from sup server")?;

    if rsp != "OK" {
        println!("{rsp}");
        std::process::exit(1);
    }

    Ok(rsp)
}
