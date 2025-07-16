use std::{
	io::{BufRead, BufReader, Write},
	os::{
		linux::net::SocketAddrExt,
		unix::net::{SocketAddr, UnixListener, UnixStream},
	},
	path::PathBuf,
	sync::{Arc, Mutex},
	time::Duration,
};

use anyhow::{Context, Result};
use log::{debug, error, warn};

use crate::{
	Cli,
	sensors::{SensorConfig, SensorInfo},
};

type CurrentConfig = Arc<Mutex<Option<SensorConfig>>>;

pub fn daemon(profiles: PathBuf) -> Result<()> {
	let current: CurrentConfig = Arc::new(Mutex::new(None));

	std::thread::spawn({
		let current = current.clone();
		move || {
			loop {
				std::thread::sleep(Duration::from_secs(15));
				if let Some(ref cfg) = *current.lock().unwrap()
					&& let Err(err) = apply_cfg(cfg)
				{
					warn!("failed to restore cfg: {err:?}");
				}
			}
		}
	});

	let socket = UnixListener::bind_addr(&SocketAddr::from_abstract_name("dev.r58playz.powerd")?)?;

	loop {
		match socket.accept() {
			Ok((socket, addr)) => {
				debug!("accepted connection from {addr:?} on unix socket");
				let profiles = profiles.clone();
				let current = current.clone();
				std::thread::spawn(move || {
					debug!(
						"handled connection from {addr:?}: {:?}",
						client(socket, profiles, current)
					)
				});
			}
			Err(err) => error!("failed to accept on unix socket: {err:?}"),
		}
	}
}

pub fn client(mut socket: UnixStream, profiles: PathBuf, current: CurrentConfig) -> Result<()> {
	let mut buf = BufReader::new(&socket);
	let mut str = String::new();
	buf.read_line(&mut str)?;

	let args = serde_json::from_str::<Cli>(&str)?;

	if let Err(err) = handle(args, &socket, profiles, current) {
		writeln!(socket, "error from daemon: {err:?}")?;
	}

	Ok(())
}

fn apply_cfg(cfg: &SensorConfig) -> Result<()> {
	let mut info = SensorInfo::read().context("failed to read current sensor data")?;
	cfg.apply(&mut info).context("failed to apply config")?;
	info.write().context("failed to write config")?;
	Ok(())
}

pub fn handle(
	args: Cli,
	mut socket: &UnixStream,
	profiles: PathBuf,
	current: CurrentConfig,
) -> Result<()> {
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

			apply_cfg(&config)?;
			current.lock().unwrap().replace(config);

			let info = SensorInfo::read()?;
			writeln!(socket, "{info}")?;
		}
		Cli::Restore => {
			if let Some(ref cfg) = *current.lock().unwrap() {
				apply_cfg(cfg)?;

				let info = SensorInfo::read()?;
				writeln!(socket, "{info}")?;
			} else {
				writeln!(socket, "no config was set")?;
			}
		}
		Cli::Daemon { profiles: _ } => {
			writeln!(socket, "no")?;
		}
	}

	Ok(())
}
