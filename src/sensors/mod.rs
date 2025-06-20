use anyhow::Result;
use cooling_profile::{CoolingProfileConfig, CoolingProfileInfo};
use intel_gpu::{GpuConfig, GpuInfo};
use intel_pstate::{PstateConfig, PstateInfo};
use intel_rapl::{RaplZoneConfig, RaplZoneInfo};
use serde::{Deserialize, Serialize};

pub mod cooling_profile;
pub mod intel_gpu;
pub mod intel_pstate;
pub mod intel_rapl;

#[derive(Clone, Debug)]
pub struct SensorInfo {
	pub rapl: Vec<RaplZoneInfo>,
	pub pstate: PstateInfo,
	pub gpus: Vec<GpuInfo>,
	pub cooling: CoolingProfileInfo,
}
impl SensorInfo {
	pub fn read() -> Result<Self> {
		Ok(Self {
			rapl: RaplZoneInfo::read_all()?,
			pstate: PstateInfo::read()?,
			gpus: GpuInfo::read_all()?,
			cooling: CoolingProfileInfo::read()?,
		})
	}

	pub fn write(&self) -> Result<()> {
		for zone in &self.rapl {
			zone.write()?;
		}

		self.pstate.write()?;

		for gpu in &self.gpus {
			gpu.write()?;
		}

		self.cooling.write()?;

		Ok(())
	}
}
impl From<SensorInfo> for SensorConfig {
	fn from(value: SensorInfo) -> Self {
		Self {
			rapl: value.rapl.into_iter().map(Into::into).collect(),
			pstate: value.pstate.into(),
			gpus: value.gpus.into_iter().map(Into::into).collect(),
			cooling: value.cooling.into(),
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SensorConfig {
	pub rapl: Vec<RaplZoneConfig>,
	pub pstate: PstateConfig,
	pub gpus: Vec<GpuConfig>,
	pub cooling: CoolingProfileConfig,
}
impl SensorConfig {
	pub fn apply(&self, info: &mut SensorInfo) -> Result<()> {
		for zone in &self.rapl {
			zone.apply(&mut info.rapl)?;
		}

		self.pstate.apply(&mut info.pstate)?;

		for gpu in &self.gpus {
			gpu.apply(&mut info.gpus)?;
		}

		self.cooling.apply(&mut info.cooling)?;

		Ok(())
	}
}
