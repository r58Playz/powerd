use std::{fmt::Display, path::PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::sysfs::{sysfs_read, sysfs_write};

#[derive(Clone, Debug)]
pub struct DptfInfo {
	tcc_offset: u64,
}

impl DptfInfo {
	pub fn read() -> Result<Self> {
		let root = PathBuf::from("bus/pci/devices/0000:00:04.0/");

		Ok(Self {
			tcc_offset: sysfs_read(&root.join("tcc_offset_degree_celsius"))?,
		})
	}
	pub fn write(&self) -> Result<()> {
		let root = PathBuf::from("bus/pci/devices/0000:00:04.0/");

		sysfs_write(&root.join("tcc_offset_degree_celsius"), self.tcc_offset)?;

		Ok(())
	}
}

impl Display for DptfInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "DPTF: tcc offset: {}degC", self.tcc_offset)
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DptfConfig {
	tcc_offset: u64,
}
impl DptfConfig {
	pub fn apply(&self, info: &mut DptfInfo) -> Result<()> {
		info.tcc_offset = self.tcc_offset;

		Ok(())
	}
}
impl From<DptfInfo> for DptfConfig {
	fn from(value: DptfInfo) -> Self {
		Self {
			tcc_offset: value.tcc_offset,
		}
	}
}
