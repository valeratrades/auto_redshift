use chrono::prelude::*;
use clap::{Args, Parser, Subcommand};
use config::AppConfig;
use std::{
	ffi::OsStr,
	process::{Command, Output},
};
use v_utils::io::ExpandedPath;
pub mod config;

#[derive(Debug, Clone, PartialEq)]
pub enum DaySection {
	Morning,
	Day,
	Evening,
	Night,
}

impl std::fmt::Display for DaySection {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			DaySection::Morning => write!(f, "morning"),
			DaySection::Day => write!(f, "day"),
			DaySection::Evening => write!(f, "evening"),
			DaySection::Night => write!(f, "night"),
		}
	}
}

/// Result of evaluating time against waketime
#[derive(Debug, Clone, PartialEq)]
pub struct TimeEvaluation {
	pub now_shifted: i32,
	pub day_section: DaySection,
	pub redshift: f32,
}

/// Pure function to evaluate time. Takes current hour/minute and waketime, returns evaluation result.
pub fn evaluate_time(current_hour: u32, current_minute: u32, waketime: &Waketime, n_hours: f32) -> TimeEvaluation {
	let nm = current_hour * 60 + current_minute;
	let wt = waketime.hours * 60 + waketime.minutes;

	let mut now_shifted = nm as i32 - wt as i32;
	if now_shifted < 0 {
		now_shifted += 24 * 60;
	}

	let day_section = match now_shifted {
		t if (t > 20 * 60) || (t <= 150) => DaySection::Morning,
		t if t <= 150 + 8 * 60 => DaySection::Day,
		t if t <= 16 * 60 => DaySection::Evening,
		_ => DaySection::Night,
	};

	let max_redshift = 20.0;
	let redshift = match &day_section {
		DaySection::Morning => 0.,
		DaySection::Day => 0.,
		DaySection::Evening => {
			if now_shifted > 12 * 60 {
				let shift_by_h = 16. - n_hours;
				let r = (now_shifted as f32 / 60.0 - shift_by_h) * (max_redshift / n_hours);
				r.min(20.0) // clamp to max
			} else {
				0.
			}
		}
		DaySection::Night => 20.,
	};

	TimeEvaluation {
		now_shifted,
		day_section,
		redshift,
	}
}

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
	let now = Utc::now();
	let eval = evaluate_time(now.hour(), now.minute(), waketime, n_hours);

	dbg!(&eval.now_shifted);

	let redshift = eval.redshift;
	let wallpaper: &str = match &eval.day_section {
		DaySection::Morning => &config.wallpapers.morning,
		DaySection::Day => &config.wallpapers.day,
		DaySection::Evening => &config.wallpapers.evening,
		DaySection::Night => &config.wallpapers.night,
	};
	let brightness_step = (config.brightness_range.1 - config.brightness_range.0) / 20.0;
	let temperature_step = (config.temperature_range.1 - config.temperature_range.0) as f32 / 20.0;

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

#[cfg(test)]
mod tests {
	use super::*;

	/// Generates a snapshot of time evaluations from 12:00 to 24:00 (midnight)
	/// with waketime set to 6:00 and n_hours = 4.0
	#[test]
	fn test_time_evaluation_12h_to_24h() {
		let waketime = Waketime::new(6, 0);
		let n_hours = 4.0;

		let mut output = String::new();
		output.push_str("Time evaluation from 12:00 to 24:00 with waketime=6:00, n_hours=4.0\n");
		output.push_str("================================================================\n\n");

		// 12:00 to 24:00 (we use 0:00 for midnight)
		for hour in 12..=23 {
			for minute in [0, 30] {
				let eval = evaluate_time(hour, minute, &waketime, n_hours);
				output.push_str(&format!(
					"{:02}:{:02} -> shifted={:4}m, section={:8}, redshift={:.2}\n",
					hour, minute, eval.now_shifted, eval.day_section, eval.redshift
				));
			}
		}
		// Add midnight (00:00)
		let eval = evaluate_time(0, 0, &waketime, n_hours);
		output.push_str(&format!(
			"{:02}:{:02} -> shifted={:4}m, section={:8}, redshift={:.2}\n",
			0, 0, eval.now_shifted, eval.day_section, eval.redshift
		));

		insta::assert_snapshot!(output, @"
		Time evaluation from 12:00 to 24:00 with waketime=6:00, n_hours=4.0
		================================================================

		12:00 -> shifted= 360m, section=day, redshift=0.00
		12:30 -> shifted= 390m, section=day, redshift=0.00
		13:00 -> shifted= 420m, section=day, redshift=0.00
		13:30 -> shifted= 450m, section=day, redshift=0.00
		14:00 -> shifted= 480m, section=day, redshift=0.00
		14:30 -> shifted= 510m, section=day, redshift=0.00
		15:00 -> shifted= 540m, section=day, redshift=0.00
		15:30 -> shifted= 570m, section=day, redshift=0.00
		16:00 -> shifted= 600m, section=day, redshift=0.00
		16:30 -> shifted= 630m, section=day, redshift=0.00
		17:00 -> shifted= 660m, section=evening, redshift=0.00
		17:30 -> shifted= 690m, section=evening, redshift=0.00
		18:00 -> shifted= 720m, section=evening, redshift=0.00
		18:30 -> shifted= 750m, section=evening, redshift=2.50
		19:00 -> shifted= 780m, section=evening, redshift=5.00
		19:30 -> shifted= 810m, section=evening, redshift=7.50
		20:00 -> shifted= 840m, section=evening, redshift=10.00
		20:30 -> shifted= 870m, section=evening, redshift=12.50
		21:00 -> shifted= 900m, section=evening, redshift=15.00
		21:30 -> shifted= 930m, section=evening, redshift=17.50
		22:00 -> shifted= 960m, section=evening, redshift=20.00
		22:30 -> shifted= 990m, section=night, redshift=20.00
		23:00 -> shifted=1020m, section=night, redshift=20.00
		23:30 -> shifted=1050m, section=night, redshift=20.00
		00:00 -> shifted=1080m, section=night, redshift=20.00
		");
	}
}
