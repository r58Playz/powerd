use std::{
	io::{Write, copy, stdout},
	os::{
		linux::net::SocketAddrExt,
		unix::net::{SocketAddr, UnixStream},
	},
	path::PathBuf,
	process::exit,
};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use log::{LevelFilter, info};
use serde::{Deserialize, Serialize};

use crate::{
	daemon::{DaemonConfig, daemon},
	sensors::{
		SensorConfig, SensorInfo,
		throttle::{cpu_throttling, graphics_throttling, ring_throttling},
	},
};

mod daemon;
mod msr;
mod sensors;
mod sysfs;
mod upower;

#[derive(ValueEnum, Copy, Clone, Deserialize, Serialize)]
enum ThrottleTarget {
	Cpu,
	Gpu,
	Ring,
}

#[derive(Parser, Deserialize, Serialize)]
enum Action {
	/// Print current info
	Info,
	/// Dump current info as a configuration
	Dump,
	/// Apply file as a configuration
	///
	/// This will override automatic profile management if using the daemon
	Apply {
		/// Path to configuration JSON
		path: PathBuf,
	},
	/// Restore automatic profile management
	Restore,
	/// Print throttling info from CPU
	ThrottleInfo { targets: Vec<ThrottleTarget> },
}

#[derive(Parser)]
enum Cli {
	#[clap(flatten)]
	Action(Action),
	/// Run action without contacting daemon
	#[command(subcommand)]
	Root(Action),
	/// Run daemon
	Daemon {
		/// Path to configuration
		config: PathBuf,
	},
}

fn main() -> Result<()> {
	env_logger::builder()
		.filter_level(LevelFilter::Trace)
		.parse_default_env()
		.init();

	let args = Cli::parse();

	match args {
		Cli::Daemon { config } => {
			info!("starting daemon");

			let cfg: DaemonConfig = serde_json::from_str(
				&std::fs::read_to_string(config).context("failed to read config file")?,
			)
			.context("failed to deserialize config")?;

			daemon(cfg)?;
		}
		Cli::Root(action) => match action {
			Action::Info => {
				println!("{}", SensorInfo::read()?);
			}
			Action::Dump => {
				println!(
					"{}",
					serde_json::to_string_pretty(&SensorConfig::from(SensorInfo::read()?))?
				);
			}
			Action::Apply { path } => {
				let cfg: SensorConfig = serde_json::from_str(
					&std::fs::read_to_string(path).context("failed to read config file")?,
				)
				.context("failed to deserialize config")?;

				let mut info = SensorInfo::read().context("failed to read current sensor data")?;
				cfg.apply(&mut info).context("failed to apply config")?;
				info.write().context("failed to write config")?;

				let info = SensorInfo::read()?;
				println!("{info}");
			}
			Action::Restore => {
				println!("restoring automatic profile management requires the daemon");
				exit(1);
			}
			Action::ThrottleInfo { targets } => {
				for target in targets {
					println!(
						"{}",
						match target {
							ThrottleTarget::Cpu => cpu_throttling()?,
							ThrottleTarget::Gpu => graphics_throttling()?,
							ThrottleTarget::Ring => ring_throttling()?,
						}
					)
				}
			}
		},
		Cli::Action(action) => {
			let serialized = serde_json::to_string(&action)?;
			let mut socket =
				UnixStream::connect_addr(&SocketAddr::from_abstract_name("dev.r58playz.powerd")?)
					.context("failed to connect to daemon")?;
			writeln!(socket, "{serialized}").context("failed to send daemon request")?;

			copy(&mut socket, &mut stdout()).context("failed to forward response")?;
		}
	}

	Ok(())
}
