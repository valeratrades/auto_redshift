use crate::{
	TimeEvaluation, apply_display_settings, calculate_display_settings,
	config::{AppConfig, LapseArgs, Waketime},
	evaluate_time, get_current_display_settings,
};

pub fn set_redshift(config: &AppConfig, redshift: f32) {
	let display = calculate_display_settings(redshift, config.steps, config.brightness_range, config.temperature_range);
	println!("redshift={redshift} -> brightness={}, temperature={}", display.brightness, display.temperature);
	apply_display_settings(&display);
}

pub fn lapse(config: &AppConfig, args: LapseArgs) {
	let initial = get_current_display_settings().expect("wl-gammarelay-rs unreachable; nothing to restore to");
	let restore = move || {
		println!("restoring brightness={:.2} temperature={:.0}K", initial.brightness, initial.temperature);
		apply_display_settings(&initial);
	};
	ctrlc::set_handler({
		let restore = restore.clone();
		move || {
			restore();
			std::process::exit(130);
		}
	})
	.expect("no other SIGINT handler is installed");
	let window = ramp(config, &args.waketime);
	let delay = args.duration.duration() / window.len() as u32;

	for (hour, minute, eval) in window {
		let display = calculate_display_settings(eval.redshift, config.steps, config.brightness_range, config.temperature_range);
		println!(
			"{hour:02}:{minute:02} section={:8} redshift={:5.2} brightness={:.2} temperature={:.0}K",
			eval.day_section, eval.redshift, display.brightness, display.temperature
		);
		apply_display_settings(&display);
		std::thread::sleep(delay);
	}
	restore();
}

fn ramp(config: &AppConfig, waketime: &Waketime) -> Vec<(u32, u32, TimeEvaluation)> {
	let day_m = 24 * 60;
	let step_m = config.update_interval_m;
	assert!(step_m > 0 && step_m <= day_m, "update_interval_m must be in 1..=1440");
	let start_m = waketime.hours * 60 + waketime.minutes;
	let mut day: Vec<_> = (0..day_m / step_m)
		.map(|i| {
			let m = (start_m + i * step_m) % day_m;
			let (hour, minute) = (m / 60, m % 60);
			(hour, minute, evaluate_time(hour, minute, waketime, config.redshift_duration, config.steps))
		})
		.collect();
	let changed = |i: &usize| day[*i].2.redshift != day[*i - 1].2.redshift;
	let first = (1..day.len()).find(changed).expect("redshift never changes over the day");
	let last = (first..day.len()).take_while(changed).last().expect("starts at a change");
	day.truncate(last + 1);
	day.drain(..first - 1);
	day
}

#[cfg(test)]
mod tests {
	use v_utils::trades::Timeframe;

	use super::*;

	#[test]
	fn ramp_ends_at_full_redshift() {
		let config = AppConfig {
			brightness_range: (0.35, 1.),
			temperature_range: (2300, 6500),
			update_interval_m: 10,
			redshift_duration: Timeframe::from("4h"),
			steps: 20,
		};
		let window = ramp(&config, &Waketime::new(7, 10));
		let (first, last) = (&window[0], window.last().unwrap());
		assert_eq!((first.0, first.1, first.2.redshift), (19, 10, 0.));
		assert_eq!((last.0, last.1, last.2.redshift), (23, 10, 20.));
	}
}
