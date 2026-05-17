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

/// State for effects that span multiple frames (gear shift, rev hold).
#[derive(Default)]
pub struct TriggerAnimations {
    prev_gear: u8,
    shift_until: Option<Instant>,
    rev_until: Option<Instant>,
}

/// Piecewise brake-pedal force curve. The lower segment is linear so
/// the player can modulate just like a real pedal; the upper segment
/// ramps steeply to `max_force` so the lock-up zone takes deliberate
/// effort to enter. `curve` shapes only the bite-zone steepness.
pub(crate) fn brake_ramp(value: u8, s: &Settings) -> f32 {
    let deadzone = s.brake_deadzone;
    let baseline = s.brake_baseline_force as f32;
    if value < deadzone {
        return baseline;
    }
    // Both spans below are clamped to ≥1 so the divides can't NaN even
    // when the user drags deadzone/bite_point/wall_at to overlap.
    let bite_point = s.brake_bite_point.max(deadzone.saturating_add(1));
    let bite_span = (bite_point - deadzone).max(1) as f32;
    let bite_force = s.brake_bite_force as f32;
    if value < bite_point {
        let r = (value - deadzone) as f32 / bite_span;
        return baseline + (bite_force - baseline) * r;
    }
    let wall_at = s.brake_wall_engage_at.max(bite_point.saturating_add(1));
    let wall_span = (wall_at - bite_point).max(1) as f32;
    let top = value.min(wall_at);
    let r = (top - bite_point) as f32 / wall_span;
    bite_force + (s.brake_max_force as f32 - bite_force) * r.powf(s.brake_curve)
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

    pub fn rev_buzz(&mut self, t: &Telemetry, s: &Settings, now: Instant) -> Option<Effect> {
        if !s.enable_rev_limiter {
            return None;
        }
        if t.accel >= s.accel_deadzone && t.max_rpm > 0.0 {
            let r = t.rpm / t.max_rpm;
            if r > s.rev_limit_ratio {
                self.rev_until = Some(
                    now + std::time::Duration::from_millis(s.rev_limit_hold_ms as u64),
                );
            }
        }
        if let Some(until) = self.rev_until {
            if now < until {
                return Some(Effect::vibration(s.rev_limit_freq, s.rev_limit_amp));
            }
        }
        None
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
        // Throttle is a flat constant force above the deadzone — a real
        // gas pedal has a uniform spring all the way through, not a
        // ramp. The wall effect at `throttle_wall_engage_at` is handled
        // separately by `r2()`.
        let force = if t.accel >= s.accel_deadzone {
            s.throttle_stiffness as f32
        } else {
            0.0
        };
        Effect::rigid(force)
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

    /// `(heavy, light)` motor intensities in 0..=255. Rumble ramps from
    /// zero at `redline_rumble_start_ratio` to `redline_rumble_max` at
    /// max RPM. Returns `(0, 0)` when telemetry is dead, so the motors
    /// stay quiet in menus and Steam keeps ownership.
    pub fn rumble(&self, t: &Telemetry, s: &Settings) -> (u8, u8) {
        if !s.enable_redline_rumble {
            return (0, 0);
        }
        let active = t.on || s.enable_idle_preview;
        if !active || t.max_rpm <= 0.0 {
            return (0, 0);
        }
        let ratio = (t.rpm / t.max_rpm).clamp(0.0, 1.0);
        let start = s.redline_rumble_start_ratio.clamp(0.0, 0.999);
        if ratio <= start {
            return (0, 0);
        }
        let span = (1.0 - start).max(1e-3);
        let k = ((ratio - start) / span).clamp(0.0, 1.0).powf(1.5);
        let level = (k * s.redline_rumble_max as f32).round() as u8;
        (level, level)
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
        if let Some(e) = self.anim.rev_buzz(t, s, now) {
            return e;
        }
        self.r2_in_wall = wall_state(t.accel, self.r2_in_wall, s.throttle_wall_engage_at, s.throttle_wall_release_at);
        if self.r2_in_wall && s.enable_throttle_resistance {
            return self.wall;
        }
        self.anim.throttle_ramp(t, s)
    }
}
