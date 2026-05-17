use serde::{Deserialize, Serialize};

/// All tunables in one place. Forces 0-255, frequencies in Hz.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // --- UDP ---
    pub udp_host: String,
    pub udp_port: u16,
    pub udp_timeout_ms: u64,

    // --- Shared pedal config ---
    pub pedal_value_max: u8,
    pub wall_zones: u8,

    // --- L2: Brake ---
    pub enable_brake_resistance: bool,
    pub brake_deadzone: u8,
    pub brake_baseline_force: u8,
    pub brake_max_force: u8,
    pub brake_curve: f32,
    pub brake_wall_engage_at: u8,
    pub brake_wall_release_at: u8,

    pub enable_handbrake_bonus: bool,
    pub handbrake_bonus: u8,

    pub enable_abs: bool,
    pub abs_brake_threshold: u8,
    pub abs_min_speed_kmh: f32,
    pub abs_slip_ratio_threshold: f32,
    pub abs_combined_slip_threshold: f32,
    pub abs_freq: u8,
    pub abs_amp: u8,

    // --- R2: Throttle ---
    pub enable_throttle_resistance: bool,
    pub accel_deadzone: u8,
    pub throttle_baseline_force: u8,
    pub throttle_max_force: u8,
    pub throttle_curve: f32,
    pub throttle_wall_engage_at: u8,
    pub throttle_wall_release_at: u8,

    pub enable_rev_limiter: bool,
    pub rev_limit_ratio: f32,
    pub rev_limit_freq: u8,
    pub rev_limit_amp: u8,
    pub rev_limit_hold_ms: f32,

    /// Drive the controller's main rumble motors based on RPM proximity
    /// to redline. Enabling this takes over the rumble bytes Steam Input
    /// would otherwise own.
    pub enable_redline_rumble: bool,
    pub redline_rumble_start_ratio: f32,
    pub redline_rumble_max: u8,

    pub enable_gear_shift: bool,
    pub enable_gear_shift_brake: bool,
    pub gear_shift_freq: u8,
    pub gear_shift_amp: u8,
    pub gear_shift_duration_ms: f32,

    // --- System ---
    pub enable_startup_pulse: bool,
    pub startup_pulse_force: u8,
    pub reconnect_interval_s: f32,
    pub exit_on_game_close: bool,
    pub game_process_name_contains: Vec<String>,
    pub game_poll_interval_s: f32,
    pub telemetry_lost_exit_s: f32,

    pub enable_auto_update: bool,

    /// When no telemetry is arriving, synthesise inputs from these
    /// sliders and run them through the real brake/throttle force
    /// curves. Useful for feeling the configured resistance without
    /// having to launch Forza.
    pub enable_test_force: bool,
    pub test_brake: u8,
    pub test_throttle: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            udp_host: "127.0.0.1".into(),
            udp_port: 5300,
            udp_timeout_ms: 500,

            pedal_value_max: 255,
            wall_zones: 2,

            enable_brake_resistance: true,
            brake_deadzone: 20,
            brake_baseline_force: 20,
            brake_max_force: 110,
            brake_curve: 2.0,
            brake_wall_engage_at: 250,
            brake_wall_release_at: 200,

            enable_handbrake_bonus: true,
            handbrake_bonus: 25,

            enable_abs: true,
            abs_brake_threshold: 80,
            abs_min_speed_kmh: 15.0,
            abs_slip_ratio_threshold: 1.0,
            abs_combined_slip_threshold: 1.0,
            abs_freq: 10,
            abs_amp: 20,

            enable_throttle_resistance: true,
            accel_deadzone: 50,
            throttle_baseline_force: 0,
            throttle_max_force: 8,
            throttle_curve: 5.0,
            throttle_wall_engage_at: 250,
            throttle_wall_release_at: 200,

            enable_rev_limiter: true,
            rev_limit_ratio: 0.93,
            rev_limit_freq: 20,
            rev_limit_amp: 1,
            rev_limit_hold_ms: 120.0,

            enable_redline_rumble: true,
            redline_rumble_start_ratio: 0.85,
            redline_rumble_max: 200,

            enable_gear_shift: true,
            enable_gear_shift_brake: true,
            gear_shift_freq: 20,
            gear_shift_amp: 255,
            gear_shift_duration_ms: 100.0,

            enable_startup_pulse: true,
            startup_pulse_force: 150,
            reconnect_interval_s: 10.0,
            exit_on_game_close: true,
            game_process_name_contains: vec!["forza".into()],
            game_poll_interval_s: 1.0,
            telemetry_lost_exit_s: 60.0,

            enable_auto_update: true,

            enable_test_force: false,
            test_brake: 0,
            test_throttle: 0,
        }
    }
}

impl Settings {
    pub fn config_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("forza-dualsense").join("settings.json"))
    }

    pub fn load_or_default() -> Self {
        if let Some(p) = Self::config_path() {
            if let Ok(s) = std::fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<Self>(&s) {
                    return v;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(p) = Self::config_path() {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(p, serde_json::to_string_pretty(self)?)?;
        }
        Ok(())
    }
}
