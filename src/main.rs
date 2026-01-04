use clap::{Args, Parser, Subcommand};
use config::AppConfig;
use jiff::Zoned;
use std::{
	ffi::OsStr,
	process::{Command, Output},
};
use tracing::info;
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

/// Actual display settings calculated from redshift value and config ranges
#[derive(Debug, Clone, PartialEq)]
pub struct DisplaySettings {
	pub temperature: f32,
	pub brightness: f32,
}

/// Calculate actual display settings from redshift value and config ranges
pub fn calculate_display_settings(redshift: f32, brightness_range: (f32, f32), temperature_range: (usize, usize)) -> DisplaySettings {
	let brightness_step = (brightness_range.1 - brightness_range.0) / 20.0;
	let temperature_step = (temperature_range.1 - temperature_range.0) as f32 / 20.0;

	let temperature = temperature_range.1 as f32 - redshift * temperature_step;
	let brightness = brightness_range.1 - redshift * brightness_step;

	DisplaySettings { temperature, brightness }
}

/// Get current display settings from wlr_gamma_service
fn get_current_display_settings() -> Option<DisplaySettings> {
	let extra_characters: &[_] = &['(', ')', ','];

	let temp_output =
		cmd("gdbus call -e -d net.zoidplex.wlr_gamma_service -o /net/zoidplex/wlr_gamma_service -m net.zoidplex.wlr_gamma_service.temperature.get");
	let temperature: f32 = String::from_utf8_lossy(&temp_output.stdout)
		.trim()
		.trim_matches(extra_characters)
		.parse()
		.ok()?;

	let bright_output =
		cmd("gdbus call -e -d net.zoidplex.wlr_gamma_service -o /net/zoidplex/wlr_gamma_service -m net.zoidplex.wlr_gamma_service.brightness.get");
	let brightness: f32 = String::from_utf8_lossy(&bright_output.stdout)
		.trim()
		.trim_matches(extra_characters)
		.parse()
		.ok()?;

	Some(DisplaySettings { temperature, brightness })
}

/// Apply display settings via wlr_gamma_service
fn apply_display_settings(display: &DisplaySettings) {
	cmd(format!(
		"gdbus call -e -d net.zoidplex.wlr_gamma_service -o /net/zoidplex/wlr_gamma_service -m net.zoidplex.wlr_gamma_service.temperature.set {} && \
		 gdbus call -e -d net.zoidplex.wlr_gamma_service -o /net/zoidplex/wlr_gamma_service -m net.zoidplex.wlr_gamma_service.brightness.set {}",
		display.temperature, display.brightness
	));
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
	/// Debug: set a specific redshift value and exit
	Dbg {
		redshift: f32,
	},
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
	v_utils::clientside!();

	let cli = Cli::parse();
	let config = config::AppConfig::read(cli.config.as_ref()).unwrap();
	match cli.command {
		Commands::Start(args) => start(config, args),
		Commands::Dbg { redshift } => dbg_set_redshift(&config, redshift),
	}
}

fn dbg_set_redshift(config: &AppConfig, redshift: f32) {
	let display = calculate_display_settings(redshift, config.brightness_range, config.temperature_range);
	println!(
		"redshift={} -> brightness={}, temperature={}",
		redshift, display.brightness, display.temperature
	);
	apply_display_settings(&display);
}

fn start(config: AppConfig, args: StartArgs) {
	let waketime = args.waketime;
	// dancing with tambourine to get into the 30m cycle
	// god forgive me
	let good_minutes_small = (waketime.minutes + 1) % 30; // +1 is offset of the cycle by 1m, to prevent bugs from having undecisive behavior on definition borders
	let good_minutes_big = good_minutes_small + 30;
	let m = Zoned::now().minute() as u32;
	let wait_to_sync_m = if m <= good_minutes_small && good_minutes_small != 0 {
		good_minutes_small - m
	} else if m <= good_minutes_big {
		good_minutes_big - m
	} else {
		good_minutes_small + 60 - m
	};
	set_redshift(&config, &waketime, args.wallpapers, args.n_hours);
	std::thread::sleep(std::time::Duration::from_secs(wait_to_sync_m as u64 * 60));
	loop {
		set_redshift(&config, &waketime, args.wallpapers, args.n_hours);
		std::thread::sleep(std::time::Duration::from_secs(30 * 60));
	}
}

fn set_redshift(config: &AppConfig, waketime: &Waketime, wallpapers: bool, n_hours: f32) {
	let now = Zoned::now();
	let eval = evaluate_time(now.hour() as u32, now.minute() as u32, waketime, n_hours);

	let wallpaper: &str = match &eval.day_section {
		DaySection::Morning => &config.wallpapers.morning,
		DaySection::Day => &config.wallpapers.day,
		DaySection::Evening => &config.wallpapers.evening,
		DaySection::Night => &config.wallpapers.night,
	};

	let target = calculate_display_settings(eval.redshift, config.brightness_range, config.temperature_range);
	let current = get_current_display_settings();

	info!(
		now_shifted = eval.now_shifted,
		section = %eval.day_section,
		redshift = eval.redshift,
		target_brightness = target.brightness,
		target_temperature = target.temperature,
		current_brightness = current.as_ref().map(|c| c.brightness),
		current_temperature = current.as_ref().map(|c| c.temperature),
		"evaluating redshift"
	);

	if eval.redshift != 0. {
		let should_apply = current
			.as_ref()
			.map(|curr| target.temperature < curr.temperature && target.brightness < curr.brightness)
			.unwrap_or(true);

		if should_apply {
			info!(brightness = target.brightness, temperature = target.temperature, "applying display settings");
			apply_display_settings(&target);
		} else {
			info!("skipping apply: current values already lower or equal");
		}
	}

	if wallpapers {
		let wallpaper_path = config.wallpapers.root.join(wallpaper);
		cmd(format!("swaymsg output '*' bg {} fill", wallpaper_path.to_str().unwrap()));
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
		// Example config ranges (typical values)
		let brightness_range = (0.5, 1.0);
		let temperature_range = (2500_usize, 6500_usize);

		let mut output = String::new();
		output.push_str("Time evaluation from 12:00 to 24:00 with waketime=6:00, n_hours=4.0\n");
		output.push_str("Config: brightness=[0.5, 1.0], temperature=[2500K, 6500K]\n");
		output.push_str("========================================================================\n\n");

		// 12:00 to 24:00 (we use 0:00 for midnight)
		for hour in 12..=23 {
			for minute in [0, 30] {
				let eval = evaluate_time(hour, minute, &waketime, n_hours);
				let display = calculate_display_settings(eval.redshift, brightness_range, temperature_range);
				output.push_str(&format!(
					"{:02}:{:02} -> section={:8}, redshift={:5.2}, temp={:6.0}K, brightness={:.2}\n",
					hour, minute, eval.day_section, eval.redshift, display.temperature, display.brightness
				));
			}
		}
		// Add midnight (00:00)
		let eval = evaluate_time(0, 0, &waketime, n_hours);
		let display = calculate_display_settings(eval.redshift, brightness_range, temperature_range);
		output.push_str(&format!(
			"{:02}:{:02} -> section={:8}, redshift={:5.2}, temp={:6.0}K, brightness={:.2}\n",
			0, 0, eval.day_section, eval.redshift, display.temperature, display.brightness
		));

		insta::assert_snapshot!(output, @r"
Time evaluation from 12:00 to 24:00 with waketime=6:00, n_hours=4.0
Config: brightness=[0.5, 1.0], temperature=[2500K, 6500K]
========================================================================

12:00 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
12:30 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
13:00 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
13:30 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
14:00 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
14:30 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
15:00 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
15:30 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
16:00 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
16:30 -> section=day, redshift= 0.00, temp=  6500K, brightness=1.00
17:00 -> section=evening, redshift= 0.00, temp=  6500K, brightness=1.00
17:30 -> section=evening, redshift= 0.00, temp=  6500K, brightness=1.00
18:00 -> section=evening, redshift= 0.00, temp=  6500K, brightness=1.00
18:30 -> section=evening, redshift= 2.50, temp=  6000K, brightness=0.94
19:00 -> section=evening, redshift= 5.00, temp=  5500K, brightness=0.88
19:30 -> section=evening, redshift= 7.50, temp=  5000K, brightness=0.81
20:00 -> section=evening, redshift=10.00, temp=  4500K, brightness=0.75
20:30 -> section=evening, redshift=12.50, temp=  4000K, brightness=0.69
21:00 -> section=evening, redshift=15.00, temp=  3500K, brightness=0.62
21:30 -> section=evening, redshift=17.50, temp=  3000K, brightness=0.56
22:00 -> section=evening, redshift=20.00, temp=  2500K, brightness=0.50
22:30 -> section=night, redshift=20.00, temp=  2500K, brightness=0.50
23:00 -> section=night, redshift=20.00, temp=  2500K, brightness=0.50
23:30 -> section=night, redshift=20.00, temp=  2500K, brightness=0.50
00:00 -> section=night, redshift=20.00, temp=  2500K, brightness=0.50
");
	}
}
