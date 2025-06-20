use std::{
	fmt::Display,
	path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::sysfs::{sysfs_exists, sysfs_read, sysfs_write};

#[derive(Clone, Debug)]
pub struct PstateCpuInfo {
	pub id: usize,
	pub hw_max_freq: u64,
	pub hw_min_freq: u64,
	pub hw_base_freq: u64,
	pub hw_current_freq: u64,

	pub governor: String,
	pub epp: String,
	pub max_freq: u64,
	pub min_freq: u64,
}
impl PstateCpuInfo {
	fn read(id: usize) -> Result<Option<Self>> {
		let root = PathBuf::from(format!("devices/system/cpu/cpu{id}/cpufreq"));

		if !sysfs_exists(&root)? {
			return Ok(None);
		}

		Ok(Some(Self {
			id,
			hw_max_freq: sysfs_read(&root.join("cpuinfo_max_freq"))?,
			hw_min_freq: sysfs_read(&root.join("cpuinfo_min_freq"))?,
			hw_base_freq: sysfs_read(&root.join("base_frequency"))?,
			hw_current_freq: sysfs_read(&root.join("scaling_cur_freq"))?,

			governor: sysfs_read(&root.join("scaling_governor"))?,
			epp: sysfs_read(&root.join("energy_performance_preference"))?,
			max_freq: sysfs_read(&root.join("scaling_max_freq"))?,
			min_freq: sysfs_read(&root.join("scaling_min_freq"))?,
		}))
	}

	fn write_min(&self, root: &Path) -> Result<()> {
		sysfs_write(&root.join("scaling_min_freq"), self.min_freq)
	}
	fn write_max(&self, root: &Path) -> Result<()> {
		sysfs_write(&root.join("scaling_max_freq"), self.max_freq)
	}

	fn write(&self) -> Result<()> {
		let root = PathBuf::from(format!("devices/system/cpu/cpu{}/cpufreq", self.id));

		sysfs_write(&root.join("scaling_governor"), &self.governor)?;
		sysfs_write(&root.join("energy_performance_preference"), &self.epp)?;

		if self.write_min(&root).is_err() {
			self.write_max(&root)?;
			self.write_min(&root)?;
		}
		self.write_max(&root)?;

		Ok(())
	}
}
impl Display for PstateCpuInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"CPU {} ({}-{}MHz, {}MHz without turbo): \"{}\" governor, \"{}\" epp, {}-{}MHz -- currently at {}MHz",
			self.id,
			self.hw_min_freq / 1000,
			self.hw_max_freq / 1000,
			self.hw_base_freq / 1000,
			&self.governor,
			&self.epp,
			self.min_freq / 1000,
			self.max_freq / 1000,
			self.hw_current_freq / 1000,
		)
	}
}

#[derive(Clone, Debug)]
pub struct PstateInfo {
	pub cpus: Vec<PstateCpuInfo>,
	pub turbo: bool,
}
impl PstateInfo {
	pub fn read() -> Result<Self> {
		let mut cpus = Vec::new();
		while let Some(cpu) = PstateCpuInfo::read(cpus.len())? {
			cpus.push(cpu);
		}

		Ok(Self {
			cpus,
			turbo: sysfs_read::<usize>(Path::new("devices/system/cpu/intel_pstate/no_turbo"))? == 0,
		})
	}

	pub fn write(&self) -> Result<()> {
		for cpu in &self.cpus {
			cpu.write()?;
		}

		sysfs_write(
			Path::new("devices/system/cpu/intel_pstate/no_turbo"),
			if self.turbo { 0 } else { 1 },
		)?;

		Ok(())
	}
}
impl Display for PstateInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Set of {} CPUs: ", self.cpus.len())?;
		if self.turbo {
			writeln!(f, "turbo enabled")?;
		} else {
			writeln!(f, "turbo disabled")?;
		}

		for cpu in &self.cpus {
			writeln!(f, "{cpu}")?;
		}

		Ok(())
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PstateCpuConfig {
	pub ids: Vec<usize>,
	pub governor: String,
	pub epp: String,
	pub max_freq: u64,
	pub min_freq: u64,
}
impl PstateCpuConfig {
	pub fn apply(&self, cpus: &mut [PstateCpuInfo]) -> Result<()> {
		for id in &self.ids {
			let cpu = cpus
				.iter_mut()
				.find(|x| x.id == *id)
				.with_context(|| format!("failed to find cpu with id {id}"))?;

			cpu.governor.clone_from(&self.governor);
			cpu.epp.clone_from(&self.epp);
			cpu.max_freq = self.max_freq;
			cpu.min_freq = self.min_freq;
		}

		Ok(())
	}
}
impl From<PstateCpuInfo> for PstateCpuConfig {
	fn from(value: PstateCpuInfo) -> Self {
		Self {
			ids: vec![value.id],
			governor: value.governor,
			epp: value.epp,
			max_freq: value.max_freq,
			min_freq: value.min_freq,
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PstateConfig {
	pub cpus: Vec<PstateCpuConfig>,
	pub turbo: bool,
}
impl PstateConfig {
	pub fn apply(&self, info: &mut PstateInfo) -> Result<()> {
		for cpu in &self.cpus {
			cpu.apply(&mut info.cpus)?;
		}

		info.turbo = self.turbo;

		Ok(())
	}
}
impl From<PstateInfo> for PstateConfig {
	fn from(value: PstateInfo) -> Self {
		Self {
			cpus: value.cpus.into_iter().map(Into::into).collect(),
			turbo: value.turbo,
		}
	}
}
