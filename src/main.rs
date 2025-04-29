use chrono::prelude::*;
use clap::{Args, Parser, Subcommand};
use config::AppConfig;
use std::{
	ffi::OsStr,
	process::{Command, Output},
};
use v_utils::io::ExpandedPath;
pub mod config;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
	#[arg(long, default_value = "~/.config/auto_redshift.toml")]
	config: ExpandedPath,
}
#[derive(Subcommand)]
enum Commands {
	Start(StartArgs),
}

#[derive(Args, Clone, Debug, Default, Copy)]
struct StartArgs {
	/// Cycle through wallpapers as day phases change
	#[arg(long)]
	wallpapers: bool,

	#[arg(long, default_value = "4.0")]
	n_hours: f32,
	waketime: Waketime,
}

#[derive(Clone, Debug, Default, derive_new::new, Copy)]
struct Waketime {
	hours: u32,
	minutes: u32,
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

fn main() {
	let cli = Cli::parse();
	let config = config::AppConfig::read(cli.config.as_ref()).unwrap();
	match cli.command {
		Commands::Start(args) => start(config, args),
	}
}

fn start(config: AppConfig, args: StartArgs) {
	let waketime = args.waketime;
	// dancing with tambourine to get into the 30m cycle
	// god forgive me
	let good_minutes_small = (waketime.minutes + 1) % 30; // +1 is offset of the cycle by 1m, to prevent bugs from having undecisive behavior on definition borders
	let good_minutes_big = good_minutes_small + 30;
	let m = Utc::now().minute();
	let mut _wait_to_sync_m = 0;
	if m <= good_minutes_small && good_minutes_small != 0 {
		_wait_to_sync_m = good_minutes_small - m;
	} else if m <= good_minutes_big {
		_wait_to_sync_m = good_minutes_big - m;
	} else {
		_wait_to_sync_m = good_minutes_small + 60 - m;
	}
	set_redshift(&config, &waketime, args.wallpapers, args.n_hours);
	std::thread::sleep(std::time::Duration::from_secs(_wait_to_sync_m as u64 * 60));
	loop {
		set_redshift(&config, &waketime, args.wallpapers, args.n_hours);
		std::thread::sleep(std::time::Duration::from_secs(30 * 60));
	}
}

fn set_redshift(config: &AppConfig, waketime: &Waketime, wallpapers: bool, n_hours: f32) {
	let nm = Utc::now().hour() * 60 + Utc::now().minute();
	let wt = waketime.hours * 60 + waketime.minutes;

	// shift everything wt minutes back
	// in python would be `(nm - wt) % 24`, but rust doesn't want to exhibit desired behaviour with % on negative numbers
	let mut now_shifted = nm as i32 - wt as i32;
	if now_shifted < 0 {
		now_shifted += 24 * 60;
	}

	dbg!(&now_shifted);
	// I guess I could be taking the day section borders as args
	let day_section: String = match now_shifted {
		t if (t > 20 * 60) || (t <= 150) => "morning".to_owned(),
		t if t <= 150 + 8 * 60 => "day".to_owned(),
		t if t <= 16 * 60 => "evening".to_owned(),
		_ => "night".to_owned(),
	};

	let redshift: f32; // redshift is a number from 0 to 20
	let max_redshift = 20.0; //TODO: switch to 0.->100. as normal people would do
	let wallpaper: &str; // wallpapers are in ~/Wallpapers
	let brightness_step = (config.brightness_range.1 - config.brightness_range.0) / 20.0;
	let temperature_step = (config.temperature_range.1 - config.temperature_range.0) as f32 / 20.0;

	match day_section.as_str() {
		"morning" => {
			redshift = 0.;
			wallpaper = &config.wallpapers.morning;
		}
		"day" => {
			redshift = 0.;
			wallpaper = &config.wallpapers.day;
		}
		"evening" => {
			if now_shifted > 12 * 60 {
				let shift_by_h = 16. - n_hours;
				redshift = (now_shifted as f32 / 60.0 - (shift_by_h)) * (max_redshift / n_hours);
				assert!(redshift <= 20.0, "redshift value is out of bounds");
			} else {
				redshift = 0.;
			}
			wallpaper = &config.wallpapers.evening;
		}
		"night" => {
			redshift = 20.;
			wallpaper = &config.wallpapers.night;
		}
		_ => unreachable!(),
	}

	if redshift != 0. {
		let temperature: f32 = config.temperature_range.1 as f32 - redshift * temperature_step;
		let brightness: f32 = config.brightness_range.1 - redshift * brightness_step;

		let extra_characters: &[_] = &['(', ')', ','];
		let current_temperature_output =
			cmd("gdbus call -e -d net.zoidplex.wlr_gamma_service -o /net/zoidplex/wlr_gamma_service -m net.zoidplex.wlr_gamma_service.temperature.get");
		let current_temperature = String::from_utf8_lossy(&current_temperature_output.stdout)
			.trim()
			.to_string()
			.trim_matches(extra_characters)
			.parse()
			.unwrap();
		let current_brightness_output =
			cmd("gdbus call -e -d net.zoidplex.wlr_gamma_service -o /net/zoidplex/wlr_gamma_service -m net.zoidplex.wlr_gamma_service.brightness.get");
		let current_brightness = String::from_utf8_lossy(&current_brightness_output.stdout)
			.trim()
			.to_string()
			.trim_matches(extra_characters)
			.parse()
			.unwrap();

		if temperature < current_temperature && brightness < current_brightness {
			let _ = cmd(format!("gdbus call -e -d net.zoidplex.wlr_gamma_service -o /net/zoidplex/wlr_gamma_service -m net.zoidplex.wlr_gamma_service.temperature.set {} && gdbus call -e -d net.zoidplex.wlr_gamma_service -o /net/zoidplex/wlr_gamma_service -m net.zoidplex.wlr_gamma_service.brightness.set {}", temperature, brightness));
		}
	}
	if wallpapers {
		let wallpaper_path = config.wallpapers.root.join(wallpaper);
		let _ = cmd(format!("swaymsg output '*' bg {} fill", wallpaper_path.to_str().unwrap()));
	}
}

fn cmd<S>(command: S) -> Output
where
	S: AsRef<OsStr>,
{
	let output = Command::new("sh").arg("-c").arg(command).output().unwrap();
	output
}
