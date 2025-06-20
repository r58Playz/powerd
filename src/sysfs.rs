use std::{error::Error, path::Path, str::FromStr};

use anyhow::{Context, Result, anyhow};

pub fn sysfs_exists(path: &Path) -> Result<bool> {
	std::fs::exists(Path::new("/sys/").join(path)).context("failed to check if sysfs path exists")
}

pub fn sysfs_read<T>(path: &Path) -> Result<T>
where
	T: FromStr,
	<T as FromStr>::Err: Error + Sync + Send + 'static,
{
	std::fs::read_to_string(Path::new("/sys/").join(path))
		.map_err(|x| anyhow!(x))
		.and_then(|x| Ok(x.trim().parse()?))
		.with_context(|| format!("failed to read sysfs {}", path.to_str().unwrap_or_default()))
}

pub fn sysfs_write(path: &Path, val: impl ToString) -> Result<()> {
	let string = val.to_string();
	std::fs::write(Path::new("/sys/").join(path), string.as_bytes()).with_context(|| {
		format!(
			"failed to write sysfs value {string:?} to {}",
			path.to_str().unwrap_or_default()
		)
	})
}
