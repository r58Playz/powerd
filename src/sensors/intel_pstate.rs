use std::{
	fmt::Display,
	path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
	msr::{Msr, msr_get_bit, msr_read, msr_set_bit, msr_write},
	sysfs::{sysfs_exists, sysfs_read, sysfs_write},
};

#[derive(Clone, Debug)]
pub struct PstateCpuInfo {
	pub id: usize,
	pub hw_max_freq: u64,
	pub hw_min_freq: u64,
	pub hw_base_freq: u64,
	pub hw_current_freq: u64,

	pub governor: String,
	pub epp: String,
	pub epb: u64,
	pub max_freq: u64,
	pub min_freq: u64,

	pub ctdp: u64,
	pub bdprochot: bool,
}
impl PstateCpuInfo {
	fn read(id: usize) -> Result<Option<Self>> {
		let root = PathBuf::from(format!("devices/system/cpu/cpu{id}/"));
		let freq = root.join("cpufreq");
		let power = root.join("power");

		if !sysfs_exists(&root)? {
			return Ok(None);
		}

		let power_ctl = msr_read(id, Msr::PowerCtl)?;

		Ok(Some(Self {
			id,
			hw_max_freq: sysfs_read(&freq.join("cpuinfo_max_freq"))?,
			hw_min_freq: sysfs_read(&freq.join("cpuinfo_min_freq"))?,
			hw_base_freq: sysfs_read(&freq.join("base_frequency"))?,
			hw_current_freq: sysfs_read(&freq.join("scaling_cur_freq"))?,

			governor: sysfs_read(&freq.join("scaling_governor"))?,
			epp: sysfs_read(&freq.join("energy_performance_preference"))?,
			epb: sysfs_read(&power.join("energy_perf_bias"))?,
			max_freq: sysfs_read(&freq.join("scaling_max_freq"))?,
			min_freq: sysfs_read(&freq.join("scaling_min_freq"))?,

			ctdp: msr_read(id, Msr::ConfigTdpControl)?,
			bdprochot: msr_get_bit(power_ctl, 0),
		}))
	}

	fn write_min(&self, root: &Path) -> Result<()> {
		sysfs_write(&root.join("scaling_min_freq"), self.min_freq)
	}
	fn write_max(&self, root: &Path) -> Result<()> {
		sysfs_write(&root.join("scaling_max_freq"), self.max_freq)
	}

	fn write(&self) -> Result<()> {
		let root = PathBuf::from(format!("devices/system/cpu/cpu{}/", self.id));
		let freq = root.join("cpufreq");
		let power = root.join("power");

		sysfs_write(&freq.join("scaling_governor"), &self.governor)?;
		sysfs_write(&freq.join("energy_performance_preference"), &self.epp)?;
		sysfs_write(&power.join("energy_perf_bias"), self.epb)?;
		msr_write(self.id, Msr::ConfigTdpControl, self.ctdp)?;

		let mut power_ctl = msr_read(self.id, Msr::PowerCtl)?;
		power_ctl = msr_set_bit(power_ctl, 0, self.bdprochot);
		msr_write(self.id, Msr::PowerCtl, power_ctl)?;

		if self.write_min(&freq).is_err() {
			self.write_max(&freq)?;
			self.write_min(&freq)?;
		}
		self.write_max(&freq)?;

		Ok(())
	}
}
impl Display for PstateCpuInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"CPU {} ({}-{}MHz, {}MHz without turbo): \"{}\" governor, \"{}\" epp, {} epb, {} cTDP, bdprochot {}, {}-{}MHz -- currently at {}MHz",
			self.id,
			self.hw_min_freq / 1000,
			self.hw_max_freq / 1000,
			self.hw_base_freq / 1000,
			&self.governor,
			&self.epp,
			self.epb,
			self.ctdp,
			if self.bdprochot {
				"enabled"
			} else {
				"disabled"
			},
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

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct PstateCpuConfig {
	pub ids: Vec<usize>,
	pub governor: String,
	pub epp: String,
	pub epb: u64,
	pub max_freq: u64,
	pub min_freq: u64,
	pub ctdp: u64,
	pub bdprochot: bool,
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
			cpu.epb = self.epb;
			cpu.max_freq = self.max_freq;
			cpu.min_freq = self.min_freq;
			cpu.ctdp = self.ctdp;
			cpu.bdprochot = self.bdprochot;
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
			epb: value.epb,
			max_freq: value.max_freq,
			min_freq: value.min_freq,
			ctdp: value.ctdp,
			bdprochot: value.bdprochot,
		}
	}
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
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
