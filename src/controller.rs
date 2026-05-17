use std::time::Instant;

use crate::settings::Settings;
use crate::telemetry::Telemetry;
use crate::triggers::{Effect, RAW_MAX};

/// Build a synthetic telemetry frame fed from the controller's actual
/// L2/R2 analog values, so idle preview drives the configured curves
/// with the user's real physical press — no magic constants.
fn synth_telemetry(brake: u8, accel: u8) -> Telemetry {
    Telemetry {
        on: true,
        brake,
        accel,
        speed_kmh: 60.0,
        rpm: 3500.0,
        max_rpm: 8000.0,
        gear: 3,
        handbrake: 0,
        ..Telemetry::default()
    }
}

/// State for effects that span multiple frames (gear-shift buzz hold).
#[derive(Default)]
pub struct TriggerAnimations {
    prev_gear: u8,
    shift_until: Option<Instant>,
}

/// Map a pedal press through the configured shape into a force value.
/// The press is normalised over `deadzone..ceiling` so the shape
/// always spans the active travel of the pedal.
pub(crate) fn pedal_force(
    value: u8,
    deadzone: u8,
    ceiling: u8,
    min: u8,
    max: u8,
    shape: crate::settings::PedalShape,
) -> f32 {
    use crate::settings::PedalShape;
    if value <= deadzone {
        // Constant is flat across the entire press range, including
        // through the deadzone — other shapes sit at the floor.
        return if shape == PedalShape::Constant {
            max as f32
        } else {
            min as f32
        };
    }
    let span = ceiling.saturating_sub(deadzone).max(1) as f32;
    let t = (((value - deadzone) as f32) / span).min(1.0);
    let y = shape.apply(t);
    min as f32 + (max as f32 - min as f32) * y
}

/// Brake force at a given press. Same math the GUI uses to draw the
/// curve preview, so what the player sees matches what they feel.
pub(crate) fn brake_ramp(value: u8, s: &Settings) -> f32 {
    pedal_force(
        value,
        s.brake_deadzone,
        s.brake_wall_engage_at,
        s.brake_min_force,
        s.brake_max_force,
        s.brake_shape,
    )
}

/// Throttle force at a given press.
pub(crate) fn throttle_force(value: u8, s: &Settings) -> f32 {
    pedal_force(
        value,
        s.accel_deadzone,
        s.throttle_wall_engage_at,
        s.throttle_min_force,
        s.throttle_max_force,
        s.throttle_shape,
    )
}

/// Returns whether the trigger is currently in its rigid "wall" zone.
/// Clamps `release_at < engage_at` so inverted slider values can't
/// invert the hysteresis (or erase it via equality).
fn wall_state(value: u8, engaged: bool, engage_at: u8, release_at: u8) -> bool {
    let release_at = release_at.min(engage_at.saturating_sub(1));
    if engaged {
        value >= release_at
    } else {
        value >= engage_at
    }
}

impl TriggerAnimations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn arm_shift(&mut self, t: &Telemetry, s: &Settings, now: Instant) {
        let gear = t.gear;
        if self.prev_gear > 0 && gear > 0 && gear != self.prev_gear && t.speed_kmh > 3.0 {
            self.shift_until = Some(
                now + std::time::Duration::from_millis(s.gear_shift_duration_ms as u64),
            );
        }
        self.prev_gear = gear;
    }

    pub fn shift_burst(&self, s: &Settings, now: Instant, pedal: u8, wall_engage_at: u8) -> Option<Effect> {
        let until = self.shift_until?;
        if now >= until {
            return None;
        }
        let half = ((wall_engage_at as u16 + RAW_MAX as u16) / 2) as u8;
        if pedal >= half {
            Some(Effect::vibration_wall(s.gear_shift_amp, s.gear_shift_freq, s.wall_zones))
        } else {
            Some(Effect::vibration(s.gear_shift_freq, s.gear_shift_amp))
        }
    }

    pub fn abs_pulse(&self, t: &Telemetry, s: &Settings) -> Option<Effect> {
        if !s.enable_abs {
            return None;
        }
        if t.brake < s.abs_brake_threshold || t.speed_kmh < s.abs_min_speed_kmh {
            return None;
        }
        if t.max_slip_ratio() < s.abs_slip_ratio_threshold
            && t.max_combined_slip() < s.abs_combined_slip_threshold
        {
            return None;
        }
        Some(Effect::vibration(s.abs_freq, s.abs_amp))
    }

    pub fn brake_resistance(&self, t: &Telemetry, s: &Settings) -> Effect {
        let handbrake = s.enable_handbrake_bonus && t.handbrake != 0;
        if !s.enable_brake_resistance {
            return if handbrake { Effect::rigid(s.handbrake_bonus as f32) } else { Effect::Off };
        }
        let mut f = brake_ramp(t.brake, s);
        if handbrake {
            f += s.handbrake_bonus as f32;
        }
        Effect::rigid(f)
    }

    pub fn throttle_ramp(&self, t: &Telemetry, s: &Settings) -> Effect {
        if !s.enable_throttle_resistance {
            return Effect::Off;
        }
        Effect::rigid(throttle_force(t.accel, s))
    }
}

pub struct Controller {
    pub anim: TriggerAnimations,
    pub wall: Effect,
    l2_in_wall: bool,
    r2_in_wall: bool,
    cached_wall_zones: u8,
}

impl Controller {
    pub fn new(s: &Settings) -> Self {
        Self {
            anim: TriggerAnimations::new(),
            wall: Effect::build_wall(s.wall_zones),
            l2_in_wall: false,
            r2_in_wall: false,
            cached_wall_zones: s.wall_zones,
        }
    }

    fn refresh_wall_if_needed(&mut self, s: &Settings) {
        if s.wall_zones != self.cached_wall_zones {
            self.wall = Effect::build_wall(s.wall_zones);
            self.cached_wall_zones = s.wall_zones;
        }
    }

    /// Produce `(L2, R2)` effects for this tick. Returns `(Off, Off)`
    /// when the race telemetry flag is off so the controller stays neutral
    /// in menus — unless the user has enabled the debug test force, in
    /// which case the synthetic brake/throttle inputs are fed through
    /// the normal force-curve pipeline so the user feels the same
    /// resistance the game would produce.
    pub fn update(
        &mut self,
        t: &Telemetry,
        s: &Settings,
        idle_press: Option<(u8, u8)>,
    ) -> (Effect, Effect) {
        if !t.on {
            if s.enable_idle_preview {
                let (l2, r2) = idle_press.unwrap_or((0, 0));
                let fake = synth_telemetry(l2, r2);
                return self.update_active(&fake, s);
            }
            return (Effect::Off, Effect::Off);
        }
        self.update_active(t, s)
    }

    fn update_active(&mut self, t: &Telemetry, s: &Settings) -> (Effect, Effect) {
        self.refresh_wall_if_needed(s);
        let now = Instant::now();
        if s.enable_gear_shift || s.enable_gear_shift_brake {
            self.anim.arm_shift(t, s, now);
        }
        let l2 = self.l2(t, s, now);
        let r2 = self.r2(t, s, now);
        (l2, r2)
    }

    fn l2(&mut self, t: &Telemetry, s: &Settings, now: Instant) -> Effect {
        if s.enable_gear_shift_brake {
            if let Some(e) = self.anim.shift_burst(s, now, t.brake, s.brake_wall_engage_at) {
                return e;
            }
        }
        if let Some(e) = self.anim.abs_pulse(t, s) {
            return e;
        }
        self.l2_in_wall = wall_state(t.brake, self.l2_in_wall, s.brake_wall_engage_at, s.brake_wall_release_at);
        if self.l2_in_wall && s.enable_brake_resistance {
            return self.wall;
        }
        self.anim.brake_resistance(t, s)
    }

    fn r2(&mut self, t: &Telemetry, s: &Settings, now: Instant) -> Effect {
        if s.enable_gear_shift {
            if let Some(e) = self.anim.shift_burst(s, now, t.accel, s.throttle_wall_engage_at) {
                return e;
            }
        }
        self.r2_in_wall = wall_state(t.accel, self.r2_in_wall, s.throttle_wall_engage_at, s.throttle_wall_release_at);
        if self.r2_in_wall && s.enable_throttle_resistance {
            return self.wall;
        }
        self.anim.throttle_ramp(t, s)
    }
}
