use std::{fmt::Display, path::Path};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::sysfs::{sysfs_read, sysfs_write};

#[derive(Debug, Clone)]
pub struct CoolingProfileInfo(String);
impl CoolingProfileInfo {
	pub fn read() -> Result<Self> {
		Ok(Self(
			sysfs_read(Path::new("firmware/acpi/platform_profile"))
				.unwrap_or_else(|_| "unknown".to_string()),
		))
	}

	pub fn write(&self) -> Result<()> {
		if &self.0 != "unknown" {
			sysfs_write(Path::new("firmware/acpi/platform_profile"), &self.0)
		} else {
			Ok(())
		}
	}
}
impl Display for CoolingProfileInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Cooling profile \"{}\"", self.0)
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoolingProfileConfig(String);
impl CoolingProfileConfig {
	pub fn apply(&self, info: &mut CoolingProfileInfo) -> Result<()> {
		info.0.clone_from(&self.0);
		Ok(())
	}
}
impl From<CoolingProfileInfo> for CoolingProfileConfig {
	fn from(value: CoolingProfileInfo) -> Self {
		Self(value.0)
	}
}
