use std::{
	fs::{File, OpenOptions},
	os::unix::fs::FileExt,
};

use anyhow::{Context, Result};

#[repr(u32)]
pub enum Msr {
	ConfigTdpControl = 0x64B,
	PowerCtl = 0x1FC,
}

fn msr_open(cpu: usize) -> Result<File> {
	OpenOptions::new()
		.read(true)
		.write(true)
		.open(format!("/dev/cpu/{cpu}/msr"))
		.context("failed to open msr")
}

pub fn msr_read(cpu: usize, reg: Msr) -> Result<u64> {
	let mut buf = [0; 8];

	let msr = msr_open(cpu)?;
	msr.read_exact_at(&mut buf, reg as u64)
		.context("failed to read msr")?;

	Ok(u64::from_ne_bytes(buf))
}

pub fn msr_write(cpu: usize, reg: Msr, val: u64) -> Result<()> {
	let buf = u64::to_ne_bytes(val);

	let msr = msr_open(cpu)?;
	msr.write_all_at(&buf, reg as u64)
		.context("failed to write msr")?;

	Ok(())
}

pub fn msr_set_bit(val: u64, bit: usize, enabled: bool) -> u64 {
	let mask = 1 << bit;

	if enabled { val | mask } else { val & !mask }
}
pub fn msr_get_bit(val: u64, bit: usize) -> bool {
	((val >> bit) & 1) == 1
}
