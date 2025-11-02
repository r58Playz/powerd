use std::time::Duration;

use anyhow::{Context, Result};
use dbus::blocking::{Connection, stdintf::org_freedesktop_dbus::Properties};

pub struct UPowerConnection {
	conn: Connection,
}
impl UPowerConnection {
	pub fn new() -> Result<Self> {
		let conn = Connection::new_system().context("failed to connect to d-bus system bus")?;
		Ok(Self { conn })
	}

	pub fn query_on_battery(&self) -> Result<bool> {
		let proxy = self.conn.with_proxy(
			"org.freedesktop.UPower",
			"/org/freedesktop/UPower",
			Duration::from_secs(1),
		);

		let on_battery: bool = proxy
			.get("org.freedesktop.UPower", "OnBattery")
			.context("failed to get org.freedesktop.UPower OnBattery")?;

		Ok(on_battery)
	}
}
