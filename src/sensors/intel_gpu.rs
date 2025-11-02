use std::{
	fmt::Display,
	path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::sysfs::{sysfs_exists, sysfs_read, sysfs_write};

#[derive(Clone, Debug)]
pub struct GpuInfo {
	pub id: usize,
	pub hw_min_freq: u64,
	pub hw_max_freq: u64,
	pub hw_eff_freq: u64,
	pub hw_cur_freq: u64,

	pub min_freq: u64,
	pub max_freq: u64,
}
impl GpuInfo {
	fn read(id: usize) -> Result<Option<Self>> {
		let root = PathBuf::from(format!("class/drm/card{id}/"));

		if !sysfs_exists(&root)? {
			return Ok(None);
		}

		Ok(Some(Self {
			id,
			hw_min_freq: sysfs_read(&root.join("gt_RPn_freq_mhz"))?,
			hw_eff_freq: sysfs_read(&root.join("gt_RP1_freq_mhz"))?,
			hw_max_freq: sysfs_read(&root.join("gt_RP0_freq_mhz"))?,
			hw_cur_freq: sysfs_read(&root.join("gt_act_freq_mhz"))?,

			min_freq: sysfs_read(&root.join("gt_min_freq_mhz"))?,
			max_freq: sysfs_read(&root.join("gt_max_freq_mhz"))?,
		}))
	}

	pub fn read_all() -> Result<Vec<Self>> {
		let mut gpus = Vec::new();
		while let Some(gpu) = Self::read(gpus.len())? {
			gpus.push(gpu);
		}
		if gpus.is_empty()
			&& let Some(gpu) = Self::read(1)?
		{
			gpus.push(gpu);
		}

		Ok(gpus)
	}

	fn write_min(&self, root: &Path) -> Result<()> {
		sysfs_write(&root.join("gt_min_freq_mhz"), self.min_freq)
	}

	fn write_max(&self, root: &Path) -> Result<()> {
		sysfs_write(&root.join("gt_max_freq_mhz"), self.max_freq)
	}

	pub fn write(&self) -> Result<()> {
		let root = PathBuf::from(format!("class/drm/card{}/", self.id));

		if self.write_min(&root).is_err() {
			self.write_max(&root)?;
			self.write_min(&root)?;
		}
		self.write_max(&root)?;

		Ok(())
	}
}
impl Display for GpuInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"GPU {} ({}-{}MHz, {}MHz efficient): {}-{}MHz -- currently at {}MHz",
			self.id,
			self.hw_min_freq,
			self.hw_max_freq,
			self.hw_eff_freq,
			self.min_freq,
			self.max_freq,
			self.hw_cur_freq,
		)
	}
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct GpuConfig {
	pub id: usize,
	pub min_freq: u64,
	pub max_freq: u64,
}
impl GpuConfig {
	pub fn apply(&self, gpus: &mut [GpuInfo]) -> Result<()> {
		let gpu = gpus
			.iter_mut()
			.find(|x| x.id == self.id)
			.with_context(|| format!("failed to find gpu with id {}", self.id))?;

		gpu.max_freq = self.max_freq;
		gpu.min_freq = self.min_freq;

		Ok(())
	}
}
impl From<GpuInfo> for GpuConfig {
	fn from(value: GpuInfo) -> Self {
		Self {
			id: value.id,
			max_freq: value.max_freq,
			min_freq: value.min_freq,
		}
	}
}
