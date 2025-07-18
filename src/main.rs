use std::{
	io::{Write, copy, stdout},
	os::{
		linux::net::SocketAddrExt,
		unix::net::{SocketAddr, UnixStream},
	},
	path::PathBuf,
};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use log::{LevelFilter, info};
use serde::{Deserialize, Serialize};

use crate::{
	daemon::daemon,
	sensors::{SensorConfig, SensorInfo}, throttle::{cpu_throttling, graphics_throttling, ring_throttling},
};

mod daemon;
mod msr;
mod sensors;
mod sysfs;
mod throttle;

#[derive(ValueEnum, Copy, Clone, Deserialize, Serialize)]
enum ThrottleTarget {
	Cpu,
	Gpu,
	Ring,
}

#[derive(Parser, Deserialize, Serialize)]
enum Cli {
	/// Print current info
	Info,
	/// Dump current info as a configuration
	Dump,
	/// Apply file as a configuration
	Apply {
		/// Path to configuration JSON
		path: PathBuf,
	},
	/// Restore current configuration
	Restore,
	/// Dump current info as a configuration without contacting daemon
	RootDump,
	/// Print current info without contacting daemon
	RootInfo,
	/// Print throttling info from CPU
	ThrottleInfo {
		targets: Vec<ThrottleTarget>,
	},
	/// Run daemon
	Daemon {
		/// Path to profiles directory
		profiles: PathBuf,
		/// Default profile to apply
		profile: Option<PathBuf>,
	},
}

fn main() -> Result<()> {
	env_logger::builder()
		.filter_level(LevelFilter::Trace)
		.parse_default_env()
		.init();

	let args = Cli::parse();

	match args {
		Cli::Daemon { profiles, profile } => {
			info!("starting daemon");

			daemon(profiles, profile)?;
		}
		Cli::RootInfo => {
			println!("{}", SensorInfo::read()?);
		}
		Cli::RootDump => {
			println!(
				"{}",
				serde_json::to_string_pretty(&SensorConfig::from(SensorInfo::read()?))?
			);
		}
		Cli::ThrottleInfo { targets } => {
			for target in targets {
				match target {
					ThrottleTarget::Cpu => cpu_throttling()?,
					ThrottleTarget::Gpu => graphics_throttling()?,
					ThrottleTarget::Ring => ring_throttling()?,
				}
			}
		}
		x => {
			let serialized = serde_json::to_string(&x)?;
			let mut socket =
				UnixStream::connect_addr(&SocketAddr::from_abstract_name("dev.r58playz.powerd")?)
					.context("failed to connect to daemon")?;
			writeln!(socket, "{serialized}").context("failed to send daemon request")?;

			copy(&mut socket, &mut stdout()).context("failed to forward response")?;
		}
	}

	Ok(())
}
