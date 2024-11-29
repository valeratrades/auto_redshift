use anyhow::Result;
use serde::{Deserialize, Deserializer};
use std::path::{Path, PathBuf};
use v_utils::macros::MyConfigPrimitives;

#[derive(Clone, Debug, Default, Deserialize, MyConfigPrimitives)]
pub struct AppConfig {
	#[serde(deserialize_with = "deserialize_tuple")]
	pub brightness_range: (f32, f32),
	#[serde(deserialize_with = "deserialize_tuple")]
	pub temperature_range: (usize, usize),
	pub wallpapers: Wallpapers,
}

#[derive(Clone, Debug, Default, derive_new::new, Deserialize)]
pub struct Wallpapers {
	pub root: PathBuf,
	pub morning: String,
	pub day: String,
	pub evening: String,
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

impl AppConfig {
	//TODO!!!: figure out how to return error iff all potentail sources combined fail to provide all of the values;
	pub fn read(path: &Path) -> Result<Self> {
		let builder = config::Config::builder().add_source(config::File::from(path.to_path_buf()));

		let settings: config::Config = builder.build()?;
		let settings: Self = settings.try_deserialize()?;

		Ok(settings)
	}
}
