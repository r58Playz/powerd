use std::{
	io::{BufRead, BufReader, Write},
	os::{
		linux::net::SocketAddrExt,
		unix::net::{SocketAddr, UnixListener, UnixStream},
	},
	path::PathBuf,
};

use anyhow::{Context, Result};
use log::{debug, error};

use crate::{
	Cli,
	sensors::{SensorConfig, SensorInfo},
};

pub fn daemon(profiles: PathBuf) -> Result<()> {
	let socket = UnixListener::bind_addr(&SocketAddr::from_abstract_name("dev.r58playz.powerd")?)?;

	loop {
		match socket.accept() {
			Ok((socket, addr)) => {
				debug!("accepted connection from {addr:?} on unix socket");
				let profiles = profiles.clone();
				std::thread::spawn(move || {
					debug!(
						"handled connection from {addr:?}: {:?}",
						client(socket, profiles)
					)
				});
			}
			Err(err) => error!("failed to accept on unix socket: {err:?}"),
		}
	}
}

pub fn client(mut socket: UnixStream, profiles: PathBuf) -> Result<()> {
	let mut buf = BufReader::new(&socket);
	let mut str = String::new();
	buf.read_line(&mut str)?;

	let args = serde_json::from_str::<Cli>(&str)?;

	if let Err(err) = handle(args, &socket, profiles) {
		writeln!(socket, "error from daemon: {err:?}")?;
	}

	Ok(())
}

pub fn handle(args: Cli, mut socket: &UnixStream, profiles: PathBuf) -> Result<()> {
	match args {
		Cli::Info => {
			writeln!(socket, "{}", SensorInfo::read()?)?;
		}
		Cli::Dump => {
			writeln!(
				socket,
				"{}",
				serde_json::to_string_pretty(&SensorConfig::from(SensorInfo::read()?))?
			)?;
		}
		Cli::Apply { path } => {
			let config: SensorConfig = serde_json::from_str(
				&std::fs::read_to_string(profiles.join(path))
					.context("failed to read config file")?,
			)
			.context("failed to deserialize config")?;

			let mut info = SensorInfo::read()?;
			config.apply(&mut info).context("failed to apply config")?;
			info.write().context("failed to write config")?;

			info = SensorInfo::read()?;
			writeln!(socket, "{info}")?;
		}
		Cli::Daemon { profiles: _ } => {
			writeln!(socket, "no")?;
		}
	}

	Ok(())
}
