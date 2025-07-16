use std::{
	io::{BufRead, BufReader, Write},
	os::{
		linux::net::SocketAddrExt,
		unix::net::{SocketAddr, UnixListener, UnixStream},
	},
	path::{Path, PathBuf},
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

fn apply_cfg_from_file(profiles: &Path, path: PathBuf, current: CurrentConfig) -> Result<()> {
	let config: SensorConfig = serde_json::from_str(
		&std::fs::read_to_string(profiles.join(path)).context("failed to read config file")?,
	)
	.context("failed to deserialize config")?;

	apply_cfg(&config)?;
	current.lock().unwrap().replace(config);

	Ok(())
}

fn apply_cfg(cfg: &SensorConfig) -> Result<()> {
	let mut info = SensorInfo::read().context("failed to read current sensor data")?;
	cfg.apply(&mut info).context("failed to apply config")?;
	info.write().context("failed to write config")?;
	Ok(())
}

pub fn daemon(profiles: PathBuf, default: Option<PathBuf>) -> Result<()> {
	let current: CurrentConfig = Arc::new(Mutex::new(None));

	if let Some(default) = default {
		let current = current.clone();

		apply_cfg_from_file(&profiles, default, current)?;
	}

	std::thread::spawn({
		let current = current.clone();
		move || {
			loop {
				std::thread::sleep(Duration::from_secs(5));
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

	if let Err(err) = handle(args, &socket, &profiles, current) {
		writeln!(socket, "error from daemon: {err:?}")?;
	}

	Ok(())
}

pub fn handle(
	args: Cli,
	mut socket: &UnixStream,
	profiles: &Path,
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
			apply_cfg_from_file(profiles, path, current)?;

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
		Cli::Daemon { .. } | Cli::RootDump | Cli::RootInfo => {
			writeln!(socket, "no")?;
		}
	}

	Ok(())
}
