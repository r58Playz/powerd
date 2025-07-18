use std::fmt::Display;

use anyhow::Result;
use cooling_profile::{CoolingProfileConfig, CoolingProfileInfo};
use intel_gpu::{GpuConfig, GpuInfo};
use intel_pstate::{PstateConfig, PstateInfo};
use intel_rapl::{RaplZoneConfig, RaplZoneInfo};
use serde::{Deserialize, Serialize};

use crate::sensors::intel_dptf::{DptfConfig, DptfInfo};

pub mod cooling_profile;
pub mod intel_dptf;
pub mod intel_gpu;
pub mod intel_pstate;
pub mod intel_rapl;

#[derive(Clone, Debug)]
pub struct SensorInfo {
	pub rapl: Vec<RaplZoneInfo>,
	pub dptf: DptfInfo,
	pub pstate: PstateInfo,
	pub gpus: Vec<GpuInfo>,
	pub cooling: CoolingProfileInfo,
}
impl SensorInfo {
	pub fn read() -> Result<Self> {
		Ok(Self {
			rapl: RaplZoneInfo::read_all()?,
			dptf: DptfInfo::read()?,
			pstate: PstateInfo::read()?,
			gpus: GpuInfo::read_all()?,
			cooling: CoolingProfileInfo::read()?,
		})
	}

	pub fn write(&self) -> Result<()> {
		for zone in &self.rapl {
			zone.write()?;
		}

		self.dptf.write()?;

		self.pstate.write()?;

		for gpu in &self.gpus {
			gpu.write()?;
		}

		self.cooling.write()?;

		Ok(())
	}
}
impl Display for SensorInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		writeln!(f, "RAPL zones:")?;
		for zone in &self.rapl {
			writeln!(f, "{zone}")?;
		}

		writeln!(f, "{}", self.dptf)?;

		writeln!(f, "{}", self.pstate)?;

		writeln!(f, "GPUs:")?;
		for gpu in &self.gpus {
			writeln!(f, "{gpu}")?;
		}

		writeln!(f, "\n{}", self.cooling)?;

		Ok(())
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SensorConfig {
	pub rapl: Vec<RaplZoneConfig>,
	pub dptf: DptfConfig,
	pub pstate: PstateConfig,
	pub gpus: Vec<GpuConfig>,
	pub cooling: CoolingProfileConfig,
}
impl SensorConfig {
	pub fn apply(&self, info: &mut SensorInfo) -> Result<()> {
		for zone in &self.rapl {
			zone.apply(&mut info.rapl)?;
		}

		self.dptf.apply(&mut info.dptf)?;

		self.pstate.apply(&mut info.pstate)?;

		for gpu in &self.gpus {
			gpu.apply(&mut info.gpus)?;
		}

		self.cooling.apply(&mut info.cooling)?;

		Ok(())
	}
}
impl From<SensorInfo> for SensorConfig {
	fn from(value: SensorInfo) -> Self {
		Self {
			rapl: value.rapl.into_iter().map(Into::into).collect(),
			dptf: value.dptf.into(),
			pstate: value.pstate.into(),
			gpus: value.gpus.into_iter().map(Into::into).collect(),
			cooling: value.cooling.into(),
		}
	}
}
