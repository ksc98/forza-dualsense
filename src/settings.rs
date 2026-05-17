use serde::{Deserialize, Serialize};

/// Pedal force "shape" — how the applied resistance varies with how
/// far the trigger is pressed. Each variant maps a normalised press
/// `t ∈ [0, 1]` to a normalised force `y ∈ [0, 1]`, which is then
/// scaled by the pedal's `min_force..max_force` envelope.
///
/// Pick the shape that matches the feel you want; both pedals expose
/// the full list so you can mix and match (e.g. flat throttle + bell
/// brake).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PedalShape {
    /// Same force across the full press. Min force is ignored.
    Constant,
    /// Force grows linearly with press.
    #[default]
    Linear,
    /// Slow build at the bottom, steep ramp at the top. The old
    /// "bite zone" feel.
    EaseIn,
    /// Heavy quickly, then plateaus.
    EaseOut,
    /// S-curve: gentle at the extremes, steep through the middle.
    EaseInOut,
    /// Peaks at mid-travel and tapers off again.
    Bell,
    /// Heavy at the start, lighter as you press through.
    Reverse,
}

impl PedalShape {
    /// `(variant, display label)` for every shape, in the order they
    /// appear in the GUI dropdown.
    pub const ALL: &'static [(PedalShape, &'static str)] = &[
        (PedalShape::Constant, "Constant (flat)"),
        (PedalShape::Linear, "Linear"),
        (PedalShape::EaseIn, "Ease in (light → heavy)"),
        (PedalShape::EaseOut, "Ease out (heavy fast → plateau)"),
        (PedalShape::EaseInOut, "Ease in-out (S-curve)"),
        (PedalShape::Bell, "Bell (light → heavy → light)"),
        (PedalShape::Reverse, "Reverse (heavy → light)"),
    ];

    /// Map normalised press `t ∈ [0, 1]` to normalised force `[0, 1]`.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            PedalShape::Constant => 1.0,
            PedalShape::Linear => t,
            PedalShape::EaseIn => t * t,
            PedalShape::EaseOut => 1.0 - (1.0 - t).powi(2),
            PedalShape::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            PedalShape::Bell => 1.0 - (2.0 * t - 1.0).powi(2),
            PedalShape::Reverse => 1.0 - t,
        }
    }
}

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
    // Force = `brake_min_force + (brake_max_force - brake_min_force) *
    //          brake_shape(t)` where `t` is the normalised press from
    // `brake_deadzone` to `brake_wall_engage_at`. Above
    // `brake_wall_engage_at` the rigid wall effect takes over.
    pub enable_brake_resistance: bool,
    pub brake_deadzone: u8,
    pub brake_shape: PedalShape,
    pub brake_min_force: u8,
    pub brake_max_force: u8,
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
    pub throttle_shape: PedalShape,
    pub throttle_min_force: u8,
    pub throttle_max_force: u8,
    pub throttle_wall_engage_at: u8,
    pub throttle_wall_release_at: u8,

    pub enable_gear_shift: bool,
    pub enable_gear_shift_brake: bool,
    pub gear_shift_freq: u8,
    pub gear_shift_amp: u8,
    pub gear_shift_duration_ms: f32,

    /// Drive the DualSense light bar like a tachometer — sweeps from
    /// green at low RPM through yellow to red at redline. Sends the
    /// LED control bits in every output report so it overrides any
    /// Steam-managed colour; turn it off to give the light bar back.
    pub enable_lightbar: bool,
    pub lightbar_brightness: u8,

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
            // Ease-in mimics the old "slow build, then steep bite zone"
            // feel — gentle through mid-travel, ramping hard at the top.
            brake_shape: PedalShape::EaseIn,
            brake_min_force: 5,
            brake_max_force: 200,
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
            // Real gas pedals are constant-tension springs. Default to
            // Constant; switch to Linear / EaseIn / Bell etc. for feel.
            throttle_shape: PedalShape::Constant,
            throttle_min_force: 0,
            throttle_max_force: 30,
            throttle_wall_engage_at: 250,
            throttle_wall_release_at: 200,

            enable_gear_shift: false,
            enable_gear_shift_brake: true,
            gear_shift_freq: 20,
            gear_shift_amp: 255,
            gear_shift_duration_ms: 100.0,

            enable_lightbar: true,
            lightbar_brightness: 200,

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
