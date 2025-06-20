use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::sysfs::{read_sysfs, sysfs_exists, write_sysfs};

#[derive(Clone, Debug)]
pub struct PstateCpuInfo {
    pub id: usize,
    pub max_hw_freq: u64,
    pub min_hw_freq: u64,
    pub max_base_freq: u64,

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
            max_hw_freq: read_sysfs(&root.join("cpuinfo_max_freq"))?,
            min_hw_freq: read_sysfs(&root.join("cpuinfo_min_freq"))?,
            max_base_freq: read_sysfs(&root.join("base_frequency"))?,

            governor: read_sysfs(&root.join("scaling_governor"))?,
            epp: read_sysfs(&root.join("energy_performance_preference"))?,
            max_freq: read_sysfs(&root.join("scaling_max_freq"))?,
            min_freq: read_sysfs(&root.join("scaling_min_freq"))?,
        }))
    }

    fn write_min_max(&self, root: &Path) -> Result<()> {
        write_sysfs(&root.join("scaling_min_freq"), self.min_freq)?;
        write_sysfs(&root.join("scaling_max_freq"), self.max_freq)?;

		Ok(())
    }

    fn write(&self) -> Result<()> {
        let root = PathBuf::from(format!("devices/system/cpu/cpu{}/cpufreq", self.id));

        write_sysfs(&root.join("scaling_governor"), &self.governor)?;
        write_sysfs(&root.join("energy_performance_preference"), &self.epp)?;

		// write thrice in case we accidentally do it in the wrong order (try to set max > min)
		self.write_min_max(&root)?;
		self.write_min_max(&root)?;
		self.write_min_max(&root)?;

        Ok(())
    }
}
impl Display for PstateCpuInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CPU {} ({}-{}MHz, {}MHz without turbo): \"{}\" governor, \"{}\" epp, {}-{}MHz",
            self.id,
            self.min_hw_freq / 1000,
            self.max_hw_freq / 1000,
            self.max_base_freq / 1000,
            &self.governor,
            &self.epp,
            self.min_freq / 1000,
            self.max_freq / 1000
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
            turbo: read_sysfs::<usize>(Path::new("devices/system/cpu/intel_pstate/no_turbo"))? == 0,
        })
    }

    pub fn write(&self) -> Result<()> {
        for cpu in &self.cpus {
            cpu.write()?;
        }

        write_sysfs(
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
