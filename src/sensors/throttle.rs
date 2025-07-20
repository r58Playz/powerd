use std::fmt::{Display, Write};

use anyhow::{Context, Result};

use crate::msr::{Msr, msr_get_bit, msr_read};

#[derive(Debug)]
enum ThrottleReason {
	Prochot,
	ThermalEvt,
	ResidencyStateRegulation,
	AvgThermalLimit,
	VrThermalEvt,
	VrTdcLimit,
	Other,
	PL1,
	PL2,
	MaxTurboLimit,
	TurboTransition,
}
impl Display for ThrottleReason {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Prochot => write!(f, "PROCHOT"),
			Self::ThermalEvt => write!(f, "Thermal event"),
			Self::ResidencyStateRegulation => write!(f, "Residency state regulation limit"),
			Self::AvgThermalLimit => write!(f, "Running average thermal limit"),
			Self::VrThermalEvt => write!(f, "Voltage regulator thermal alert"),
			Self::VrTdcLimit => write!(f, "Voltage regulator TDC limit"),
			Self::Other => write!(f, "Other/electrical/EDP"),
			Self::PL1 => write!(f, "PL1"),
			Self::PL2 => write!(f, "PL2"),
			Self::MaxTurboLimit => write!(f, "Max turbo limit"),
			Self::TurboTransition => write!(f, "Turbo transition attenuation"),
		}
	}
}

fn print(ty: &str, reasons: Vec<ThrottleReason>) -> Result<String> {
	let mut out = String::new();

	write!(out, "{ty} throttle reasons: ")?;
	if reasons.is_empty() {
		write!(out, "None")?;
	} else {
		write!(
			out,
			"{}",
			reasons
				.into_iter()
				.map(|x| x.to_string())
				.collect::<Vec<_>>()
				.join(", ")
		)?;
	}

	Ok(out)
}

pub fn cpu_throttling() -> Result<String> {
	let msr =
		msr_read(1, Msr::CpuPerfLimitReasons).context("failed to read cpu throttle reasons")?;

	let mut reasons = Vec::new();

	macro_rules! check {
		($bit:expr, $ty:tt) => {
			if msr_get_bit(msr, $bit) {
				reasons.push(ThrottleReason::$ty);
			}
		};
	}

	check!(0, Prochot);
	check!(1, ThermalEvt);
	check!(4, ResidencyStateRegulation);
	check!(5, AvgThermalLimit);
	check!(6, VrThermalEvt);
	check!(7, VrTdcLimit);
	check!(8, Other);
	check!(10, PL1);
	check!(11, PL2);
	check!(12, MaxTurboLimit);
	check!(13, TurboTransition);

	print("CPU", reasons)
}

pub fn graphics_throttling() -> Result<String> {
	let msr = msr_read(1, Msr::GraphicsPerfLimitReasons)
		.context("failed to read graphics throttle reasons")?;

	let mut reasons = Vec::new();

	macro_rules! check {
		($bit:expr, $ty:tt) => {
			if msr_get_bit(msr, $bit) {
				reasons.push(ThrottleReason::$ty);
			}
		};
	}

	check!(0, Prochot);
	check!(1, ThermalEvt);
	check!(5, AvgThermalLimit);
	check!(6, VrThermalEvt);
	check!(7, VrTdcLimit);
	check!(8, Other);
	check!(10, PL1);
	check!(11, PL2);

	let mut out = print("GPU", reasons)?;
	if msr_get_bit(msr, 12) {
		write!(out, "\nGPU operating below target frequency")?;
	}

	Ok(out)
}

pub fn ring_throttling() -> Result<String> {
	let msr =
		msr_read(1, Msr::RingPerfLimitReasons).context("failed to read ring throttle reasons")?;

	let mut reasons = Vec::new();

	macro_rules! check {
		($bit:expr, $ty:tt) => {
			if msr_get_bit(msr, $bit) {
				reasons.push(ThrottleReason::$ty);
			}
		};
	}

	check!(0, Prochot);
	check!(1, ThermalEvt);
	check!(5, AvgThermalLimit);
	check!(6, VrThermalEvt);
	check!(7, VrTdcLimit);
	check!(8, Other);
	check!(10, PL1);
	check!(11, PL2);

	print("Ring", reasons)
}
