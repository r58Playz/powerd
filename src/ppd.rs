use std::{
	collections::HashMap,
	fmt::Display,
	path::Path,
	str::FromStr,
	sync::{
		Arc, Mutex,
		mpsc::{self, TryRecvError, channel},
	},
	time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use dbus::{
	arg::Variant, blocking::Connection, channel::{MatchingReceiver, Sender}, message::MatchRule, strings::BusName, Message, MethodErr
};
use dbus_crossroads::{Crossroads, IfaceBuilder};
use log::warn;
use serde::{Deserialize, Serialize};

use crate::daemon::{CurrentState, DaemonConfig, PowerProfilesDaemonProfiles, apply_cfg_from_file};

const POWER_PROFILES_DAEMON_NAME: &str = "org.freedesktop.UPower.PowerProfiles";
const POWER_PROFILES_DAEMON_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const POWER_PROFILES_DAEMON_VERSION: &str = "0.30.0";

#[derive(Debug, Copy, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PpdProfile {
	#[serde(rename = "power-saver")]
	PowerSaver,
	Balanced,
	Performance,
}
impl Display for PpdProfile {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::PowerSaver => write!(f, "power-saver"),
			Self::Balanced => write!(f, "balanced"),
			Self::Performance => write!(f, "performance"),
		}
	}
}
impl FromStr for PpdProfile {
	type Err = anyhow::Error;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"power-saver" => Ok(Self::PowerSaver),
			"balanced" => Ok(Self::Balanced),
			"performance" => Ok(Self::Performance),
			x => Err(anyhow!("invalid power-profiles-daemon profile: {x}",)),
		}
	}
}
impl PpdProfile {
	fn ser(&self) -> HashMap<String, Variant<String>> {
		let mut map = HashMap::with_capacity(3);

		map.insert("Profile".to_string(), Variant(self.to_string()));
		map.insert("PlatformDriver".to_string(), Variant("powerd".to_string()));
		map.insert("Driver".to_string(), Variant("powerd".to_string()));

		map
	}

	fn into_powerd(self, ppd: &PowerProfilesDaemonProfiles) -> &Path {
		match self {
			Self::PowerSaver => &ppd.powersave,
			Self::Balanced => &ppd.balanced,
			Self::Performance => &ppd.performance,
		}
	}
}
const ALL_PROFILES: [PpdProfile; 3] = [
	PpdProfile::PowerSaver,
	PpdProfile::Balanced,
	PpdProfile::Performance,
];

struct ProfileHold {
	cookie: u32,

	profile: PpdProfile,
	reason: String,
	application_id: String,

	sender: BusName<'static>,
}
impl ProfileHold {
	fn ser(&self) -> HashMap<String, Variant<String>> {
		let mut map = HashMap::with_capacity(3);

		map.insert("ApplicationId".to_string(), Variant(self.application_id.clone()));
		map.insert("Profile".to_string(), Variant(self.profile.to_string()));
		map.insert("Reason".to_string(), Variant(self.reason.clone()));

		map
	}
}

enum PpdMessage {
	ProfileChanged {
		profile: PpdProfile,
		external: bool,
		invalidate_holds: bool,
	},
}

struct PpdState {
	next_cookie: u32,
	holds: Vec<ProfileHold>,
	ppd: mpsc::Sender<PpdMessage>,
	daemon: mpsc::Sender<()>,

	cfg: DaemonConfig,
	state: CurrentState,
}
impl PpdState {
	fn calculate_holds(&mut self) -> Result<()> {
		use PpdProfile as P;
		let mut target = None;

		for hold in &self.holds {
			let profile = match (hold.profile, target) {
				(P::PowerSaver, _) => P::PowerSaver,
				(P::Performance, None | Some(P::Performance)) => P::Performance,
				(P::Performance, Some(P::PowerSaver)) => P::PowerSaver,
				(_, Some(P::Balanced)) => unreachable!("balanced profile not allowed"),
				(P::Balanced, _) => unreachable!("balanced profile not allowed"),
			};
			target = Some(profile);
		}

		if let Some(target) = target {
			self.set_profile(target, true, true)?;
		} else {
			let mut current = self.state.lock().unwrap();
			current.held.take();
			drop(current);

			self.daemon.send(()).context("failed to notify daemon")?;
		}

		Ok(())
	}

	fn hold_profile(
		&mut self,
		message: &Message,
		profile: PpdProfile,
		reason: String,
		application_id: String,
	) -> Result<u32> {
		let cookie = self.next_cookie;
		self.next_cookie += 1;

		if profile == PpdProfile::Balanced {
			bail!("balanced profile not allowed to be held");
		}

		self.holds.push(ProfileHold {
			cookie,
			profile,
			reason,
			application_id,
			sender: message
				.sender()
				.context("failed to get sender of message")?
				.into_static(),
		});

		self.calculate_holds()
			.context("failed to recalculate hold state")?;

		Ok(cookie)
	}

	fn release_profile(&mut self, cookie: u32) -> Result<()> {
		self.holds.retain(|x| x.cookie != cookie);

		self.calculate_holds()
			.context("failed to recalculate hold state")?;

		Ok(())
	}

	fn get_profile(&mut self) -> PpdProfile {
		self.state.lock().unwrap().ppd_profile
	}

	fn set_profile(&mut self, profile: PpdProfile, external: bool, from_hold: bool) -> Result<()> {
		let powerd = profile.into_powerd(&self.cfg.ppd);

		let mut current = self.state.lock().unwrap();
		let state = apply_cfg_from_file(&self.cfg.profiles, powerd)?;
		current.ppd_set = true;
		current.ppd_profile = state.cfg.ppd_name;
		if from_hold {
			current.held.replace(state);
		} else {
			current.manual.replace(state);
		}

		self.ppd
			.send(PpdMessage::ProfileChanged {
				profile,
				external,
				invalidate_holds: !from_hold,
			})
			.context("failed to tell daemon profile changed")?;

		Ok(())
	}
}

#[derive(Clone)]
pub struct PowerProfilesDaemon(mpsc::Sender<PpdMessage>);

impl PowerProfilesDaemon {
	fn daemon(
		rx: mpsc::Receiver<PpdMessage>,
		tx: mpsc::Sender<PpdMessage>,
		daemon: mpsc::Sender<()>,
		conn: Connection,
		cfg: DaemonConfig,
		daemon_state: CurrentState,
	) -> Result<()> {
		let mut cr = Crossroads::new();

		let state = Arc::new(Mutex::new(PpdState {
			next_cookie: 0,
			holds: Vec::new(),
			ppd: tx,
			daemon,

			cfg,
			state: daemon_state,
		}));

		let mut released_signal = None;
		let mut changed_fn = None;

		let iface = cr.register(
			POWER_PROFILES_DAEMON_NAME,
			|b: &mut IfaceBuilder<Arc<Mutex<PpdState>>>| {
				b.method(
					"HoldProfile",
					("profile", "reason", "application_id"),
					("cookie",),
					|cx, state, (profile, reason, application_id): (String, String, String)| {
						let profile = match PpdProfile::from_str(&profile) {
							Ok(x) => x,
							Err(_) => return Err(MethodErr::invalid_arg(&profile)),
						};
						match state.lock().unwrap().hold_profile(
							cx.message(),
							profile,
							reason,
							application_id,
						) {
							Ok(x) => Ok((x,)),
							Err(err) => Err(MethodErr::failed(&err)),
						}
					},
				);
				b.method(
					"ReleaseProfile",
					("cookie",),
					(),
					|_, state, (cookie,): (u32,)| match state
						.lock()
						.unwrap()
						.release_profile(cookie)
					{
						Ok(()) => Ok(()),
						Err(err) => Err(MethodErr::failed(&err)),
					},
				);
				released_signal.replace(
					b.signal::<(u32,), _>("ProfileReleased", ("cookie",))
						.msg_fn(),
				);

				b.property("ActiveProfileHolds").get(|_, state| {
					Ok(state
						.lock()
						.unwrap()
						.holds
						.iter()
						.map(|x| x.ser())
						.collect::<Vec<_>>())
				});

				changed_fn.replace(
					b.property("ActiveProfile")
						.get(|_, state| Ok(state.lock().unwrap().get_profile().to_string()))
						.set(|_, state, profile| {
							let parsed = match PpdProfile::from_str(&profile) {
								Ok(x) => x,
								Err(_) => return Err(MethodErr::invalid_arg(&profile)),
							};
							match state.lock().unwrap().set_profile(parsed, false, true) {
								Ok(()) => Ok(Some(profile)),
								Err(err) => Err(MethodErr::failed(&err)),
							}
						})
						.emits_changed_true()
						.changed_msg_fn(),
				);

				b.property("PerformanceInhibited")
					.deprecated()
					.get(|_, _| Ok(String::new()));
				b.property("PerformanceDegraded")
					.get(|_, _| Ok(String::new()));

				b.property("Profiles")
					.get(|_, _| Ok(ALL_PROFILES.iter().map(|x| x.ser()).collect::<Vec<_>>()));

				// powerd doesn't support any actions
				b.property("Actions").get(|_, _| Ok(Vec::<String>::new()));
				b.property("ActionsInfo")
					.get(|_, _| Ok(Vec::<HashMap<String, Variant<String>>>::new()));
				b.method(
					"SetActionEnabled",
					("action", "enabled"),
					(),
					|_, _, _: (String, bool)| Ok(()),
				);

				b.property("Version")
					.get(|_, _| Ok(POWER_PROFILES_DAEMON_VERSION.to_string()));

				b.property("BatteryAware")
					.get(|_, state| Ok(state.lock().unwrap().cfg.default.is_some()))
					.set(|_, _, _| Ok(None));
			},
		);

		cr.insert(POWER_PROFILES_DAEMON_PATH, &[iface], state.clone());

		let Some(released_signal) = released_signal else {
			unreachable!("released_signal not set");
		};
		let Some(changed_fn) = changed_fn else {
			unreachable!("changed_fn not set");
		};

		conn.start_receive(
			MatchRule::new_method_call(),
			Box::new(move |msg, conn| {
				cr.handle_message(msg, conn).unwrap();
				true
			}),
		);
		conn.start_receive(
			MatchRule::new()
				.with_interface("org.freedesktop.DBus")
				.with_member("NameOwnerChanged")
				.with_path("/org/freedesktop/DBus"),
			Box::new({
				let state = state.clone();
				move |msg, _| {
					if let (Some(name), Some(old_owner), Some(new_owner)) =
						msg.get3::<String, String, String>()
						&& new_owner.is_empty()
						&& !old_owner.is_empty()
					{
						// client gone
						let mut current = state.lock().unwrap();

						if let Some(holder) = current
							.holds
							.iter()
							.find(|x| x.sender == name.as_str().into())
							.map(|x| x.cookie) && let Err(err) = current.release_profile(holder)
						{
							warn!("failed to release profile after client left: {err:?}");
						}
					}
					true
				}
			}),
		);

		loop {
			conn.process(Duration::from_millis(100))?;

			loop {
				match rx.try_recv() {
					Ok(PpdMessage::ProfileChanged {
						profile,
						external,
						invalidate_holds,
					}) => {
						if external
							&& let Some(msg) =
								changed_fn(&POWER_PROFILES_DAEMON_PATH.into(), &profile.to_string())
						{
							let _ = conn.send(msg);
						}

						if invalidate_holds {
							let mut current = state.lock().unwrap();

							for hold in current.holds.drain(..) {
								let mut msg = released_signal(
									&POWER_PROFILES_DAEMON_PATH.into(),
									&(hold.cookie,),
								);
								msg.set_destination(Some(hold.sender));
								let _ = conn.send(msg);
							}
						}
					}
					Err(TryRecvError::Empty) => break,
					Err(TryRecvError::Disconnected) => return Ok(()),
				}
			}
		}
	}

	pub fn new(cfg: DaemonConfig, state: CurrentState, daemon: mpsc::Sender<()>) -> Result<Self> {
		let (tx, rx) = channel();

		let conn = Connection::new_system().context("failed to connect to system bus")?;
		conn.request_name(POWER_PROFILES_DAEMON_NAME, false, true, false)
			.context("failed to request ppd name")?;

		std::thread::spawn({
			let tx = tx.clone();
			move || {
				if let Err(err) = Self::daemon(rx, tx, daemon, conn, cfg, state) {
					warn!("power-profiles-daemon polyfill exited: {err:?}");
				}
			}
		});

		Ok(Self(tx))
	}

	pub fn profile_changed(&self, profile: PpdProfile) -> Result<()> {
		self.0
			.send(PpdMessage::ProfileChanged {
				profile,
				external: true,
				invalidate_holds: true,
			})
			.context("failed to notify ppd daemon")
	}
}
