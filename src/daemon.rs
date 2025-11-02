use std::{
	io::{BufRead, BufReader, Write},
	os::{
		linux::net::SocketAddrExt,
		unix::net::{SocketAddr, UnixListener, UnixStream},
	},
	path::{Path, PathBuf},
	sync::{
		Arc, Mutex,
		mpsc::{RecvTimeoutError, Sender, channel},
	},
	time::Duration,
};

use anyhow::{Context, Result};
use log::{debug, error, warn};
use serde::Deserialize;

use crate::{
	Action, ThrottleTarget,
	ppd::{PowerProfilesDaemon, PpdProfile},
	sensors::{
		SensorConfig, SensorInfo,
		throttle::{cpu_throttling, graphics_throttling, ring_throttling},
	},
	upower::UPowerConnection,
};

#[derive(Clone, Deserialize)]
pub struct DefaultProfiles {
	pub ac: PathBuf,
	pub battery: PathBuf,
}
#[derive(Clone, Deserialize)]
pub struct PowerProfilesDaemonProfiles {
	#[serde(rename = "power-saver")]
	pub powersave: PathBuf,
	pub balanced: PathBuf,
	pub performance: PathBuf,
}

#[derive(Clone, Deserialize)]
pub struct DaemonConfig {
	pub profiles: PathBuf,
	pub default: Option<DefaultProfiles>,
	pub ppd: PowerProfilesDaemonProfiles,
	pub poll_frequency: Option<u64>,
}

#[derive(Eq, PartialEq, Clone)]
pub struct ProfileInfo {
	pub cfg: SensorConfig,
	pub path: PathBuf,
}

pub struct CurrentProfile {
	pub held: Option<ProfileInfo>,
	pub manual: Option<ProfileInfo>,
	pub ppd_profile: PpdProfile,
	pub ppd_set: bool,
}
impl CurrentProfile {
	pub fn get_override(&self) -> Option<&ProfileInfo> {
		self.held.as_ref().or(self.manual.as_ref())
	}
}

pub type CurrentState = Arc<Mutex<CurrentProfile>>;

fn read_cfg(profiles: &Path, path: &Path) -> Result<ProfileInfo> {
	let path = profiles.join(path);
	let cfg: SensorConfig = serde_json::from_str(
		&std::fs::read_to_string(&path).context("failed to read config file")?,
	)
	.context("failed to deserialize config")?;

	Ok(ProfileInfo { cfg, path })
}

pub fn apply_cfg_from_file(profiles: &Path, path: &Path) -> Result<ProfileInfo> {
	let info = read_cfg(profiles, path)?;

	apply_cfg(&info.cfg)?;

	Ok(info)
}

fn apply_cfg(cfg: &SensorConfig) -> Result<()> {
	let mut info = SensorInfo::read().context("failed to read current sensor data")?;
	cfg.apply(&mut info).context("failed to apply config")?;
	info.write().context("failed to write config")?;
	Ok(())
}

pub fn daemon(cfg: DaemonConfig) -> Result<()> {
	let current: CurrentState = Arc::new(Mutex::new(CurrentProfile {
		held: None,
		manual: None,
		ppd_profile: PpdProfile::Balanced,
		ppd_set: false,
	}));

	let (tx, rx) = channel::<()>();

	let ppd = PowerProfilesDaemon::new(cfg.clone(), current.clone(), tx.clone())
		.context("failed to start ppd polyfill")?;

	let poll_frequency = cfg.poll_frequency.unwrap_or(30);
	std::thread::spawn({
		let upower = UPowerConnection::new()?;
		let cfg = cfg.clone();
		let current = current.clone();
		let tx = tx.clone();
		move || {
			// immediately wake on init
			let _ = tx.send(());

			let mut manual = None;
			let mut held = None;

			loop {
				match rx.recv_timeout(Duration::from_secs(poll_frequency)) {
					Ok(()) | Err(RecvTimeoutError::Timeout) => {}
					Err(RecvTimeoutError::Disconnected) => return,
				}

				let mut current = current.lock().unwrap();

				let ppd_profile = if let Some(cfg) = current.get_override() {
					if let Err(err) = apply_cfg(&cfg.cfg) {
						warn!("failed to restore cfg: {err:?}");
					}
					Some(cfg.cfg.ppd_name)
				} else if let Some(default) = &cfg.default {
					match upower.query_on_battery() {
						Ok(on_battery) => {
							let path = if on_battery {
								&default.battery
							} else {
								&default.ac
							};

							match apply_cfg_from_file(&cfg.profiles, path) {
								Ok(info) => Some(info.cfg.ppd_name),
								Err(err) => {
									warn!("failed to apply default config: {err:?}");
									None
								}
							}
						}
						Err(err) => {
							warn!("failed to ask upower for battery stats: {err:?}");
							None
						}
					}
				} else {
					None
				};

				// Check if state changed (override added/removed/changed)
				let state_changed = manual != current.manual || held != current.held;

				if state_changed {
					manual.clone_from(&current.manual);
					held.clone_from(&current.held);

					if let Some(ppd_profile) = ppd_profile {
						current.ppd_profile = ppd_profile;
						current.ppd_set = false; // Mark as externally changed
						drop(current);

						if let Err(err) = ppd.profile_changed(ppd_profile) {
							warn!("failed to tell ppd daemon that profile changed: {err:?}");
						}
					} else {
						drop(current);
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
				let tx = tx.clone();
				std::thread::spawn(move || {
					debug!(
						"handled connection from {addr:?}: {:?}",
						client(socket, profiles, current, tx)
					)
				});
			}
			Err(err) => error!("failed to accept on unix socket: {err:?}"),
		}
	}
}

fn client(
	mut socket: UnixStream,
	profiles: PathBuf,
	current: CurrentState,
	tx: Sender<()>,
) -> Result<()> {
	let mut buf = BufReader::new(&socket);
	let mut str = String::new();
	buf.read_line(&mut str)?;

	let args = serde_json::from_str::<Action>(&str)?;

	if let Err(err) = handle(args, &socket, &profiles, current, tx) {
		writeln!(socket, "error from daemon: {err:?}")?;
	}

	Ok(())
}

fn handle(
	action: Action,
	mut socket: &UnixStream,
	profiles: &Path,
	current: CurrentState,
	tx: Sender<()>,
) -> Result<()> {
	match action {
		Action::Info => {
			let current = current.lock().unwrap();
			let held = current.held.as_ref().map(|x| x.path.clone());
			let manual = current.manual.as_ref().map(|x| x.path.clone());

			if let Some(path) = held {
				writeln!(socket, "PPD held profile: {path:?}")?;
			} else {
				writeln!(socket, "No PPD held profile")?;
			}
			if let Some(path) = manual {
				writeln!(socket, "Manual profile override: {path:?}")?;
			} else {
				writeln!(socket, "No manual profile override set")?;
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
			let info = apply_cfg_from_file(profiles, &path)?;
			let mut current = current.lock().unwrap();
			current.ppd_profile = info.cfg.ppd_name;
			current.ppd_set = false;
			current.manual.replace(info);
			current.held.take();
			drop(current);
			let _ = tx.send(());

			let info = SensorInfo::read()?;
			writeln!(socket, "{info}")?;
		}
		Action::Restore => {
			let mut current = current.lock().unwrap();
			current.manual.take();
			current.ppd_set = false;
			drop(current);
			let _ = tx.send(());
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
