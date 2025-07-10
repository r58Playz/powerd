use std::{
	io::{Write, copy, stdout},
	os::{
		linux::net::SocketAddrExt,
		unix::net::{SocketAddr, UnixStream},
	},
	path::PathBuf,
};

use anyhow::Result;
use clap::Parser;
use log::{info, LevelFilter};
use serde::{Deserialize, Serialize};

use crate::daemon::daemon;

mod daemon;
mod sensors;
mod sysfs;

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
	Daemon {
		/// Path to profiles directory
		profiles: PathBuf,
	},
}

fn main() -> Result<()> {
	let args = Cli::parse();

	match args {
		Cli::Daemon { profiles } => {
			env_logger::builder()
				.filter_level(LevelFilter::Trace)
				.parse_default_env()
				.init();

			info!("starting daemon");

			daemon(profiles)?;
		}
		x => {
			let serialized = serde_json::to_string(&x)?;
			let mut socket =
				UnixStream::connect_addr(&SocketAddr::from_abstract_name("dev.r58playz.powerd")?)?;
			writeln!(socket, "{serialized}")?;

			copy(&mut socket, &mut stdout())?;
		}
	}

	Ok(())
}
