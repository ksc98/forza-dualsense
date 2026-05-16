use serde::Serialize;

/// Parsed Forza Horizon "Dash" UDP packet. We only keep the fields the
/// trigger controller actually reads — everything else is parsed but
/// discarded.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Telemetry {
    pub on: bool,
    pub timestamp_ms: u32,

    pub max_rpm: f32,
    pub idle_rpm: f32,
    pub rpm: f32,

    pub tire_slip_ratio: [f32; 4],     // fl, fr, rl, rr
    pub tire_combined_slip: [f32; 4],  // fl, fr, rl, rr

    pub speed_kmh: f32,
    pub power_w: f32,
    pub torque_nm: f32,

    pub gear: u8,
    pub accel: u8,
    pub brake: u8,
    pub clutch: u8,
    pub handbrake: u8,
    pub steer: i8,
}

#[inline]
fn f32_at(buf: &[u8], off: usize) -> f32 {
    f32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

#[inline]
fn u32_at(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

impl Telemetry {
    /// Parse a Forza Horizon "Dash" 324-byte packet. Returns None if the
    /// packet is too short to contain the dash extension. Field offsets
    /// match the published Forza spec exactly.
    pub fn parse(buf: &[u8]) -> Option<Self> {
        // The dash format runs from offset 0 through 322 (inclusive); we
        // accept the documented 324-byte packet but only strictly need
        // through byte 322.
        if buf.len() < 323 {
            return None;
        }

        let on = u32_at(buf, 0) != 0;
        let timestamp_ms = u32_at(buf, 4);

        let max_rpm = f32_at(buf, 8);
        let idle_rpm = f32_at(buf, 12);
        let rpm = f32_at(buf, 16);

        // tire_slip_ratio fl,fr,rl,rr at offsets 84..100
        let tire_slip_ratio = [
            f32_at(buf, 84),
            f32_at(buf, 88),
            f32_at(buf, 92),
            f32_at(buf, 96),
        ];

        // tire_combined_slip fl,fr,rl,rr at offsets 180..196
        let tire_combined_slip = [
            f32_at(buf, 180),
            f32_at(buf, 184),
            f32_at(buf, 188),
            f32_at(buf, 192),
        ];

        // Horizon-specific dash fields begin after the 12-byte gap at 232.
        // speed (m/s) at 256, power (W) at 260, torque (Nm) at 264.
        let speed_ms = f32_at(buf, 256);
        let speed_kmh = speed_ms * 3.6;
        let power_w = f32_at(buf, 260);
        let torque_nm = f32_at(buf, 264);

        // Single-byte controls at the tail (315..323).
        let accel = buf[315];
        let brake = buf[316];
        let clutch = buf[317];
        let handbrake = buf[318];
        let gear = buf[319];
        let steer = buf[320] as i8;

        Some(Self {
            on,
            timestamp_ms,
            max_rpm,
            idle_rpm,
            rpm,
            tire_slip_ratio,
            tire_combined_slip,
            speed_kmh,
            power_w,
            torque_nm,
            gear,
            accel,
            brake,
            clutch,
            handbrake,
            steer,
        })
    }

    #[inline]
    pub fn max_slip_ratio(&self) -> f32 {
        self.tire_slip_ratio.iter().map(|v| v.abs()).fold(0.0_f32, f32::max)
    }

    #[inline]
    pub fn max_combined_slip(&self) -> f32 {
        self.tire_combined_slip.iter().map(|v| v.abs()).fold(0.0_f32, f32::max)
    }
}
