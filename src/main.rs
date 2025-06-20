use anyhow::Result;
use sensors::{SensorConfig, SensorInfo};

mod sensors;
mod sysfs;

fn main() -> Result<()> {
	let info = SensorInfo::read()?;
	info.write()?;

	let config = serde_json::to_string_pretty(&SensorConfig::from(info))?;
	println!("{config}");

	Ok(())
}
