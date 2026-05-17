use serde::{Deserialize, Serialize};

/// All tunables in one place. Forces 0-255, frequencies in Hz.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // --- UDP ---
    pub udp_host: String,
    pub udp_port: u16,

    // --- Shared pedal config ---
    pub wall_zones: u8,

    // --- L2: Brake ---
    //
    // The brake trigger uses a piecewise force curve:
    //   * 0..deadzone:            no contact, baseline force only
    //   * deadzone..bite_point:   linear modulation — real pedal feel
    //   * bite_point..wall_at:    steep ramp toward `max_force` — the
    //                              "bite" zone you have to mean to push
    //                              through (mimics anti-lock onset)
    //   * wall_at..:              rigid wall, full lock-up
    pub enable_brake_resistance: bool,
    pub brake_deadzone: u8,
    pub brake_baseline_force: u8,
    pub brake_bite_point: u8,
    pub brake_bite_force: u8,
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
    /// Single linear-stiffness control for the throttle. Force at the
    /// wall edge equals this value; below the wall it ramps linearly
    /// from 0 at the deadzone. A real gas pedal is linear — one knob
    /// matches the mental model and avoids fatigue from sustained
    /// non-linear top-end force.
    pub throttle_stiffness: u8,
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

    pub enable_auto_update: bool,

    /// When no telemetry is arriving, drive both triggers as if the car
    /// were on-track at a mid-pedal press. Lets you feel the configured
    /// resistance while tuning without having to launch Forza.
    pub enable_idle_preview: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            udp_host: "127.0.0.1".into(),
            udp_port: 5300,

            wall_zones: 2,

            enable_brake_resistance: true,
            brake_deadzone: 10,
            brake_baseline_force: 5,
            // At ~80% physical press the player should feel ~50% of the
            // lockup force — slow linear build, then a steep curved
            // ramp to full stiffness over the final 20% of travel.
            brake_bite_point: 204,
            brake_bite_force: 100,
            brake_max_force: 200,
            brake_curve: 2.5,
            brake_wall_engage_at: 250,
            brake_wall_release_at: 220,

            enable_handbrake_bonus: true,
            handbrake_bonus: 25,

            enable_abs: false,
            abs_brake_threshold: 80,
            abs_min_speed_kmh: 15.0,
            abs_slip_ratio_threshold: 1.0,
            abs_combined_slip_threshold: 1.0,
            abs_freq: 10,
            abs_amp: 20,

            enable_throttle_resistance: true,
            accel_deadzone: 50,
            throttle_stiffness: 30,
            throttle_wall_engage_at: 250,
            throttle_wall_release_at: 200,

            enable_rev_limiter: true,
            rev_limit_ratio: 0.93,
            rev_limit_freq: 20,
            rev_limit_amp: 1,
            rev_limit_hold_ms: 120.0,

            enable_redline_rumble: false,
            redline_rumble_start_ratio: 0.85,
            redline_rumble_max: 200,

            enable_gear_shift: false,
            enable_gear_shift_brake: false,
            gear_shift_freq: 20,
            gear_shift_amp: 255,
            gear_shift_duration_ms: 100.0,

            enable_startup_pulse: true,
            startup_pulse_force: 150,

            enable_auto_update: true,

            enable_idle_preview: true,
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

    /// Atomic write: serialise, write to a sibling tmp file, then rename
    /// into place. Prevents a crash or concurrent writer from leaving a
    /// truncated settings.json on disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let Some(p) = Self::config_path() else {
            return Ok(());
        };
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }
}
