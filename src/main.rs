use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use sensors::{SensorConfig, SensorInfo};

mod sensors;
mod sysfs;

#[derive(Parser)]
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
}

fn main() -> Result<()> {
	let args = Cli::parse();

	match args {
		Cli::Info => {
			println!("{}", SensorInfo::read()?);
		},
		Cli::Dump => {
			println!(
				"{}",
				serde_json::to_string_pretty(&SensorConfig::from(SensorInfo::read()?))?
			)
		}
		Cli::Apply { path } => {
			let config: SensorConfig = serde_json::from_str(
				&std::fs::read_to_string(path).context("failed to read config file")?,
			)
			.context("failed to deserialize config")?;

			let mut info = SensorInfo::read()?;
			config.apply(&mut info).context("failed to apply config")?;
			info.write().context("failed to write config")?;

			info = SensorInfo::read()?;
			println!("{info}");
		}
	}

	Ok(())
}
