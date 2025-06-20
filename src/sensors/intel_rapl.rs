use std::{
	fmt::Display,
	path::{Path, PathBuf},
	time::Duration,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::sysfs::{sysfs_exists, sysfs_read, sysfs_write};

#[derive(Clone, Debug)]
pub struct RaplConstraintInfo {
	pub id: usize,
	pub name: String,
	pub power_limit: u64,
	pub time_window: Option<Duration>,
}
impl RaplConstraintInfo {
	fn read(zone_path: &Path, id: usize) -> Result<Option<Self>> {
		let name = zone_path.join(format!("constraint_{id}_name"));

		if !sysfs_exists(&name)? {
			return Ok(None);
		}

		Ok(Some(Self {
			id,
			name: sysfs_read(&name)?,
			power_limit: sysfs_read(&zone_path.join(format!("constraint_{id}_power_limit_uw")))?,
			time_window: sysfs_read(&zone_path.join(format!("constraint_{id}_time_window_us")))
				.ok()
				.map(Duration::from_micros),
		}))
	}

	fn write(&self, zone_path: &Path) -> Result<()> {
		let id = self.id;
		sysfs_write(
			&zone_path.join(format!("constraint_{id}_power_limit_uw")),
			self.power_limit,
		)?;

		if let Some(time_window) = &self.time_window {
			sysfs_write(
				&zone_path.join(format!("constraint_{id}_time_window_us")),
				time_window.as_micros(),
			)?;
		}

		Ok(())
	}
}
impl Display for RaplConstraintInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"Constraint \"{}\": {}W",
			self.name,
			self.power_limit / 1000000
		)?;
		if let Some(time_window) = self.time_window {
			write!(f, " over a time window of {time_window:?}")?;
		} else {
			write!(f, " over no time window")?;
		}

		Ok(())
	}
}

#[derive(Clone, Debug)]
pub struct RaplZoneInfo {
	pub path: PathBuf,
	pub name: String,
	pub constraints: Vec<RaplConstraintInfo>,
	pub subzones: Vec<RaplZoneInfo>,
}
impl RaplZoneInfo {
	fn read_zone(zone_path: PathBuf) -> Result<Option<Self>> {
		if !sysfs_exists(&zone_path)? {
			return Ok(None);
		}

		let mut constraints = Vec::new();
		while let Some(constraint) = RaplConstraintInfo::read(&zone_path, constraints.len())? {
			constraints.push(constraint);
		}

		let mut subzones = Vec::new();
		let zone_name = zone_path
			.file_name()
			.context("unable to get rapl zone name")?
			.to_str()
			.context("invalid rapl zone name")?;
		while let Some(subzone) =
			RaplZoneInfo::read_zone(zone_path.join(format!("{zone_name}:{}", subzones.len())))?
		{
			subzones.push(subzone);
		}

		Ok(Some(Self {
			name: sysfs_read(&zone_path.join("name"))?,
			path: zone_path,
			constraints,
			subzones,
		}))
	}

	pub fn write(&self) -> Result<()> {
		for constraint in &self.constraints {
			constraint.write(&self.path)?;
		}

		for subzone in &self.subzones {
			subzone.write()?;
		}

		Ok(())
	}

	pub fn read_all() -> Result<Vec<Self>> {
		let root = Path::new("devices/virtual/powercap/intel-rapl/");

		let mut zones = Vec::new();
		while let Some(subzone) =
			RaplZoneInfo::read_zone(root.join(format!("intel-rapl:{}", zones.len())))?
		{
			zones.push(subzone);
		}

		Ok(zones)
	}
}
impl Display for RaplZoneInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		writeln!(
			f,
			"Zone \"{}\": {} constraints, {} subzones",
			self.name,
			self.constraints.len(),
			self.subzones.len()
		)?;

		for constraint in &self.constraints {
			writeln!(f, "{constraint}")?;
		}

		for subzone in &self.subzones {
			write!(f, "{subzone}")?;
		}

		Ok(())
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RaplConstraintConfig {
	pub id: usize,
	pub power_limit: Option<u64>,
	pub time_window: Option<u64>,
}
impl RaplConstraintConfig {
	pub fn apply(&self, constraints: &mut [RaplConstraintInfo]) -> Result<()> {
		let constraint = constraints
			.iter_mut()
			.find(|x| x.id == self.id)
			.with_context(|| format!("failed to find constraint with id {}", self.id))?;

		if let Some(power_limit) = self.power_limit {
			constraint.power_limit = power_limit;
		}
		if let Some(time_window) = self.time_window {
			constraint.time_window = Some(Duration::from_micros(time_window));
		}

		Ok(())
	}
}
impl From<RaplConstraintInfo> for RaplConstraintConfig {
	fn from(value: RaplConstraintInfo) -> Self {
		Self {
			id: value.id,
			power_limit: Some(value.power_limit),
			time_window: value.time_window.map(|x| x.as_micros() as u64),
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RaplZoneConfig {
	pub name: String,
	pub constraints: Vec<RaplConstraintConfig>,
	pub subzones: Vec<RaplZoneConfig>,
}
impl RaplZoneConfig {
	pub fn apply(&self, zones: &mut [RaplZoneInfo]) -> Result<()> {
		let zone_info = zones
			.iter_mut()
			.find(|x| x.name == self.name)
			.with_context(|| format!("failed to find zone with name {}", self.name))?;

		for zone in &self.subzones {
			zone.apply(&mut zone_info.subzones)?;
		}

		for constraint in &self.constraints {
			constraint.apply(&mut zone_info.constraints)?;
		}

		Ok(())
	}
}
impl From<RaplZoneInfo> for RaplZoneConfig {
	fn from(value: RaplZoneInfo) -> Self {
		Self {
			name: value.name,
			constraints: value.constraints.into_iter().map(Into::into).collect(),
			subzones: value.subzones.into_iter().map(Into::into).collect(),
		}
	}
}
