use std::{fmt::Display, path::Path};

use anyhow::Result;

use crate::sysfs::{read_sysfs, write_sysfs};

#[derive(Debug)]
pub struct CoolingProfileInfo(String);
impl CoolingProfileInfo {
    pub fn read() -> Result<Self> {
        Ok(Self(read_sysfs(Path::new(
            "firmware/acpi/platform_profile",
        ))?))
    }

    pub fn write(&self) -> Result<()> {
        write_sysfs(Path::new("firmware/acpi/platform_profile"), &self.0)
    }
}
impl Display for CoolingProfileInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cooling profile \"{}\"", self.0)
    }
}
