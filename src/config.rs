use clap::Parser;
use serde::{Deserialize, Deserializer};
use std::path::PathBuf;
use v_utils::macros::{LiveSettings, MyConfigPrimitives, Settings, SettingsNested};

/// Controls which brightness backend to use
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BrightnessBackend {
	/// Software gamma via wlr_gamma_service (gdbus) - the original method
	#[default]
	WlrGamma,
	/// Hardware backlight via brightnessctl
	Brightnessctl,
}

#[derive(Clone, Debug, LiveSettings, MyConfigPrimitives, Settings)]
pub struct AppConfig {
	#[serde(deserialize_with = "deserialize_tuple")]
	#[settings(skip(flag))]
	pub brightness_range: (f32, f32),
	#[serde(deserialize_with = "deserialize_tuple")]
	#[settings(skip(flag))]
	pub temperature_range: (usize, usize),
	/// Which backend to use for brightness control. Defaults to wlr_gamma (software gamma).
	#[serde(default)]
	#[settings(skip(flag))]
	#[primitives(skip)]
	pub brightness_backend: BrightnessBackend,
	#[settings(flatten)]
	pub wallpapers: Wallpapers,
	/// Minutes between brightness adjustments
	#[settings(default = 10)]
	pub update_interval_m: u32,
}

#[derive(Clone, Debug, smart_default::SmartDefault, MyConfigPrimitives, SettingsNested)]
pub struct Wallpapers {
	pub root: PathBuf,
	#[default = ""]
	pub morning: String,
	#[default = ""]
	pub day: String,
	#[default = ""]
	pub evening: String,
	#[default = ""]
	pub night: String,
}

fn deserialize_tuple<'de, D, T>(deserializer: D) -> Result<(T, T), D::Error>
where
	D: Deserializer<'de>,
	T: Deserialize<'de> + Copy,
{
	let vec = Vec::<T>::deserialize(deserializer)?;
	if vec.len() != 2 {
		return Err(serde::de::Error::custom("Expected a tuple of 2 elements"));
	}
	Ok((vec[0], vec[1]))
}

/// CLI struct with SettingsFlags for clap integration
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
	#[clap(flatten)]
	pub settings_flags: SettingsFlags,
	#[command(subcommand)]
	pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
	Start(StartArgs),
	/// Debug: set a specific redshift value and exit
	Dbg {
		redshift: f32,
	},
}

#[derive(clap::Args, Clone, Debug, Default, Copy)]
pub struct StartArgs {
	/// Cycle through wallpapers as day phases change
	#[arg(long)]
	pub wallpapers: bool,

	#[arg(long, default_value = "4.0")]
	pub n_hours: f32,
	pub waketime: Waketime,
}

#[derive(Clone, Debug, Default, derive_new::new, Copy)]
pub struct Waketime {
	pub hours: u32,
	pub minutes: u32,
}
impl From<String> for Waketime {
	fn from(s: String) -> Self {
		let split: Vec<_> = s.split(':').collect();
		assert!(split.len() == 2, "ERROR: waketime should be supplied in the format: \"%H:%M\"");
		let hours = split[0].parse().unwrap();
		let minutes = split[1].parse().unwrap();
		Waketime { hours, minutes }
	}
}
