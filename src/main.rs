use anyhow::Result;
use sensors::{cooling_profile::CoolingProfileInfo, intel_gpu::GpuInfo, intel_pstate::PstateInfo, intel_rapl::RaplZoneInfo};

mod sysfs;
mod sensors;

fn main() -> Result<()> {
	println!("RAPL Zones:");
	let rapl = RaplZoneInfo::read_all()?;
	for zone in rapl {
		println!("{zone}");
		zone.write()?;
	}

	let cooling = CoolingProfileInfo::read()?;
	println!("{cooling}\n");
	cooling.write()?;

    let cpu = PstateInfo::read()?;
    println!("{cpu}");
    cpu.write()?;

	let gpus = GpuInfo::read_all()?;
	for gpu in gpus {
		println!("{gpu}");
		gpu.write()?;
	}

	Ok(())
}
