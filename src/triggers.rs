//! DualSense adaptive trigger effect primitives.
//!
//! Each `Effect` serializes to exactly 11 bytes: a 1-byte mode followed
//! by 10 parameter bytes. That block lives at the per-trigger offset
//! inside the output HID report. Mode bytes match the values the
//! controller firmware accepts.

use serde::Serialize;

pub const M_OFF: u8 = 0x05;
pub const M_RIGID: u8 = 0x01;
pub const M_PULSE: u8 = 0x06;
pub const M_FEEDBACK: u8 = 0x21; // MultiplePositionFeedback
pub const M_PULSE_AB: u8 = 0x26; // Pulse_AB

pub const RAW_MAX: u8 = 255;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    Off,
    Rigid { force: u8 },
    Vibration { freq: u8, amp: u8 },
    VibrationWall { freq: u8, amp_strength: u8, wall_zones: u8 },
    Feedback { zones: [u8; 10] },
}

#[inline]
fn clamp_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

#[inline]
fn clamp_round(v: f32) -> u8 {
    let r = v.round() as i32;
    clamp_u8(r)
}

impl Effect {
    pub fn rigid(force: f32) -> Self {
        Effect::Rigid { force: clamp_round(force) }
    }

    pub fn vibration(freq: u8, amp: u8) -> Self {
        Effect::Vibration { freq, amp }
    }

    /// `amp_byte` is the 0..=255 amplitude as configured; we collapse it
    /// to a 1..=8 strength bucket for Pulse_AB.
    pub fn vibration_wall(amp_byte: u8, freq: u8, wall_zones: u8) -> Self {
        let s = ((amp_byte as i32 / 32) + 1).clamp(1, 8) as u8;
        let w = wall_zones.clamp(1, 9);
        Effect::VibrationWall { freq, amp_strength: s, wall_zones: w }
    }

    /// Coarse 0..=1 magnitude for visualisation. Not used by the HID path.
    pub fn display_force(&self) -> f32 {
        match *self {
            Effect::Off => 0.0,
            Effect::Rigid { force } => force as f32 / 255.0,
            Effect::Vibration { amp, .. } => amp as f32 / 255.0,
            Effect::VibrationWall { amp_strength, .. } => amp_strength as f32 / 8.0,
            Effect::Feedback { zones } => {
                let max = zones.iter().copied().max().unwrap_or(0);
                max as f32 / 8.0
            }
        }
    }

    /// Static firmware wall — top `n` zones (1..=9) maxed at strength 8.
    pub fn build_wall(zones: u8) -> Self {
        let n = zones.clamp(1, 9) as usize;
        let mut z = [0u8; 10];
        for slot in z.iter_mut().take(10).skip(10 - n) {
            *slot = 8;
        }
        Effect::Feedback { zones: z }
    }

    /// Serialize to (mode, 10 parameter bytes).
    #[allow(clippy::wrong_self_convention)]
    pub fn to_hid_bytes(&self) -> (u8, [u8; 10]) {
        let mut out = [0u8; 10];
        match *self {
            Effect::Off => (M_OFF, out),
            Effect::Rigid { force } => {
                out[0] = 0;
                out[1] = force;
                (M_RIGID, out)
            }
            Effect::Vibration { freq, amp } => {
                out[0] = freq;
                out[1] = amp;
                (M_PULSE, out)
            }
            Effect::VibrationWall { freq, amp_strength, wall_zones } => {
                let a = amp_strength.clamp(1, 8) as u32;
                let w = wall_zones.clamp(1, 9) as usize;

                // Lower (10 - w) zones at strength `a`, top w zones at 8.
                let mut zones = [0u8; 10];
                for slot in zones.iter_mut().take(10 - w) {
                    *slot = a as u8;
                }
                for slot in zones.iter_mut().skip(10 - w) {
                    *slot = 8;
                }

                let (active, strength) = encode_zones(&zones);
                out[0] = (active & 0xFF) as u8;
                out[1] = ((active >> 8) & 0xFF) as u8;
                out[2] = (strength & 0xFF) as u8;
                out[3] = ((strength >> 8) & 0xFF) as u8;
                out[4] = ((strength >> 16) & 0xFF) as u8;
                out[5] = ((strength >> 24) & 0xFF) as u8;
                out[6] = freq;
                (M_PULSE_AB, out)
            }
            Effect::Feedback { zones } => {
                let (active, strength) = encode_zones(&zones);
                out[0] = (active & 0xFF) as u8;
                out[1] = ((active >> 8) & 0xFF) as u8;
                out[2] = (strength & 0xFF) as u8;
                out[3] = ((strength >> 8) & 0xFF) as u8;
                out[4] = ((strength >> 16) & 0xFF) as u8;
                out[5] = ((strength >> 24) & 0xFF) as u8;
                (M_FEEDBACK, out)
            }
        }
    }
}

/// 10 zones, 1 bit each in `active`, 3 bits each in `strength`.
/// A zone strength of 0 leaves the zone inactive; otherwise the encoded
/// strength is `(s - 1)` so strengths 1..=8 fit in 3 bits.
fn encode_zones(zones: &[u8; 10]) -> (u16, u32) {
    let mut active: u16 = 0;
    let mut strength: u32 = 0;
    for (i, &z) in zones.iter().enumerate() {
        let s = z.clamp(0, 8) as u32;
        if s > 0 {
            active |= 1 << i;
            strength |= (s - 1) << (3 * i);
        }
    }
    (active, strength)
}
