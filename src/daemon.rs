use std::{
	io::{BufRead, BufReader, Write}, os::{
		linux::net::SocketAddrExt,
		unix::net::{SocketAddr, UnixListener, UnixStream},
	}, path::{Path, PathBuf}, sync::{Arc, Mutex}, time::Duration
};

use anyhow::{Context, Result};
use log::{debug, error, warn};
use serde::Deserialize;

use crate::{
	Action, ThrottleTarget,
	sensors::{
		SensorConfig, SensorInfo,
		throttle::{cpu_throttling, graphics_throttling, ring_throttling},
	},
	upower::UPowerConnection,
};

#[derive(Clone, Deserialize)]
struct DefaultProfiles {
	ac: PathBuf,
	battery: PathBuf,
}

#[derive(Clone, Deserialize)]
pub struct DaemonConfig {
	profiles: PathBuf,
	default: DefaultProfiles,
}

struct CurrentProfile {
	cfg: SensorConfig,
	path: PathBuf,
}

type CurrentState = Arc<Mutex<Option<CurrentProfile>>>;

fn apply_cfg_from_file(profiles: &Path, path: &Path) -> Result<CurrentProfile> {
	let path = profiles.join(path);
	let cfg: SensorConfig = serde_json::from_str(
		&std::fs::read_to_string(&path).context("failed to read config file")?,
	)
	.context("failed to deserialize config")?;

	apply_cfg(&cfg)?;

	Ok(CurrentProfile { cfg, path })
}

fn apply_cfg(cfg: &SensorConfig) -> Result<()> {
	let mut info = SensorInfo::read().context("failed to read current sensor data")?;
	cfg.apply(&mut info).context("failed to apply config")?;
	info.write().context("failed to write config")?;
	Ok(())
}

pub fn daemon(cfg: DaemonConfig) -> Result<()> {
	let current: CurrentState = Arc::new(Mutex::new(None));

	std::thread::spawn({
		let upower = UPowerConnection::new()?;
		let cfg = cfg.clone();
		let current = current.clone();
		move || {
			loop {
				std::thread::sleep(Duration::from_secs(5));

				let current = current.lock().unwrap();

				if let Some(ref cfg) = *current {
					if let Err(err) = apply_cfg(&cfg.cfg) {
						warn!("failed to restore cfg: {err:?}");
					}
				} else {
					match upower.query_on_battery() {
						Ok(on_battery) => {
							let path = if on_battery {
								&cfg.default.battery
							} else {
								&cfg.default.ac
							};

							if let Err(err) = apply_cfg_from_file(&cfg.profiles, path) {
								warn!("failed to apply default config: {err:?}");
							}
						}
						Err(err) => warn!("failed to ask upower for battery stats: {err:?}"),
					}
				}
			}
		}
	});

	let socket = UnixListener::bind_addr(&SocketAddr::from_abstract_name("dev.r58playz.powerd")?)?;

	loop {
		match socket.accept() {
			Ok((socket, addr)) => {
				debug!("accepted connection from {addr:?} on unix socket");
				let profiles = cfg.profiles.clone();
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

fn client(mut socket: UnixStream, profiles: PathBuf, current: CurrentState) -> Result<()> {
	let mut buf = BufReader::new(&socket);
	let mut str = String::new();
	buf.read_line(&mut str)?;

	let args = serde_json::from_str::<Action>(&str)?;

	if let Err(err) = handle(args, &socket, &profiles, current) {
		writeln!(socket, "error from daemon: {err:?}")?;
	}

	Ok(())
}

fn handle(
	action: Action,
	mut socket: &UnixStream,
	profiles: &Path,
	current: CurrentState,
) -> Result<()> {
	match action {
		Action::Info => {
			let path = current.lock().unwrap().as_ref().map(|x| x.path.clone());
			if let Some(path) = path {
				writeln!(socket, "Profile override: {path:?}")?;
			} else {
				writeln!(socket, "No profile override set")?;
			}
			writeln!(socket, "\n{}", SensorInfo::read()?)?;
		}
		Action::Dump => {
			writeln!(
				socket,
				"{}",
				serde_json::to_string_pretty(&SensorConfig::from(SensorInfo::read()?))?
			)?;
		}
		Action::Apply { path } => {
			let cfg = apply_cfg_from_file(profiles, &path)?;
			current.lock().unwrap().replace(cfg);

			let info = SensorInfo::read()?;
			writeln!(socket, "{info}")?;
		}
		Action::Restore => {
			current.lock().unwrap().take();
		}
		Action::ThrottleInfo { targets } => {
			for target in targets {
				writeln!(
					socket,
					"{}",
					match target {
						ThrottleTarget::Cpu => cpu_throttling()?,
						ThrottleTarget::Gpu => graphics_throttling()?,
						ThrottleTarget::Ring => ring_throttling()?,
					}
				)?;
			}
		}
	}

	Ok(())
}
