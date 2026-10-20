use crate::{
	apply_display_settings, calculate_display_settings,
	config::{AppConfig, LapseArgs},
	evaluate_time,
};

pub fn set_redshift(config: &AppConfig, redshift: f32) {
	let display = calculate_display_settings(redshift, config.brightness_range, config.temperature_range);
	println!("redshift={redshift} -> brightness={}, temperature={}", display.brightness, display.temperature);
	apply_display_settings(&display);
}

pub fn lapse(config: &AppConfig, args: LapseArgs) {
	let day_m = 24 * 60;
	let step_m = config.update_interval_m;
	assert!(step_m > 0 && step_m <= day_m, "update_interval_m must be in 1..=1440");
	let steps = day_m / step_m;
	let delay = args.duration.duration() / steps;
	let start_m = args.waketime.hours * 60 + args.waketime.minutes;

	for i in 0..steps {
		let m = (start_m + i * step_m) % day_m;
		let (hour, minute) = (m / 60, m % 60);
		let eval = evaluate_time(hour, minute, &args.waketime, args.n_hours);
		let display = calculate_display_settings(eval.redshift, config.brightness_range, config.temperature_range);
		println!(
			"{hour:02}:{minute:02} section={:8} redshift={:5.2} brightness={:.2} temperature={:.0}K",
			eval.day_section, eval.redshift, display.brightness, display.temperature
		);
		apply_display_settings(&display);
		std::thread::sleep(delay);
	}
}
