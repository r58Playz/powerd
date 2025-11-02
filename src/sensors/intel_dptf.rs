use std::{
	fmt::Display,
	path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::sysfs::{sysfs_exists, sysfs_read, sysfs_write};

const DPTF_DEVICES: &[&str] = &[
	"INT3400:00",
	"INTC1040:00",
	"INTC1041:00",
	"INTC10A0:00",
	"INTC1042:00",
	"INTC1068:00",
	"INTC10D4:00",
];

#[derive(Clone, Debug)]
pub struct DptfInfo {
	intxx_path: PathBuf,

	uuid: String,
	uuids: Vec<String>,

	tcc_offset: u64,
}

impl DptfInfo {
	pub fn read() -> Result<Self> {
		let intxx_base = PathBuf::from("/sys/bus/platform/drivers/int3400 thermal");
		let intxx_path = DPTF_DEVICES
			.iter()
			.map(|x| intxx_base.join(x))
			.find(|x| sysfs_exists(x).is_ok_and(|x| x))
			.context("failed to find intxx device")?;

		let uuid = sysfs_read(&intxx_path.join("uuids/current_uuid"))?;
		let uuids = sysfs_read::<String>(&intxx_path.join("uuids/available_uuids"))?
			.lines()
			.map(ToOwned::to_owned)
			.collect();

		Ok(Self {
			intxx_path,
			uuid,
			uuids,
			tcc_offset: sysfs_read(Path::new(
				"bus/pci/devices/0000:00:04.0/tcc_offset_degree_celsius",
			))?,
		})
	}

	pub fn write(&self) -> Result<()> {
		sysfs_write(&self.intxx_path.join("uuids/current_uuid"), &self.uuid)?;

		sysfs_write(
			Path::new("bus/pci/devices/0000:00:04.0/tcc_offset_degree_celsius"),
			self.tcc_offset,
		)?;

		Ok(())
	}
}

impl Display for DptfInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		writeln!(f, "DPTF:")?;
		writeln!(f, "tcc offset: {}degC", self.tcc_offset)?;
		writeln!(f, "available uuids: {:?}", self.uuids)?;
		write!(f, "current uuid: {:?}", self.uuid)
	}
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct DptfConfig {
	tcc_offset: u64,

	uuid: String,
}
impl DptfConfig {
	pub fn apply(&self, info: &mut DptfInfo) -> Result<()> {
		info.tcc_offset = self.tcc_offset;

		info.uuid = self.uuid.clone();

		Ok(())
	}
}
impl From<DptfInfo> for DptfConfig {
	fn from(value: DptfInfo) -> Self {
		Self {
			tcc_offset: value.tcc_offset,
			uuid: value.uuid,
		}
	}
}
