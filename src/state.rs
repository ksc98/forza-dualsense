use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use serde::Serialize;

use crate::logs::SharedLogs;
use crate::settings::Settings;
use crate::telemetry::Telemetry;
use crate::update::Status as UpdateStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HidStatus {
    Disconnected,
    Connected,
    Error,
}

pub struct AppState {
    pub settings: Settings,
    pub telemetry: Telemetry,
    pub hid_status: HidStatus,
    pub hid_transport: Option<crate::hid::Transport>,
    pub hid_serial: String,
    pub hid_error: String,
    pub packets_received: u64,
    pub last_packet_at: Option<Instant>,
    pub started_at: Instant,
    pub udp_bound: bool,
    pub web_url: String,
    pub last_settings_save_error: String,
    pub update_status: UpdateStatus,
    pub logs: SharedLogs,
    /// Latest analog L2/R2 readings from the controller's HID input
    /// report, if any. Populated by the HID worker and read by the GUI
    /// so the live-position cursor on the curve graph follows the
    /// physical trigger when no game telemetry is arriving.
    pub last_trigger_input: Option<(u8, u8)>,
}

impl AppState {
    pub fn new(settings: Settings, logs: SharedLogs) -> Self {
        Self {
            settings,
            logs,
            telemetry: Telemetry::default(),
            hid_status: HidStatus::Disconnected,
            hid_transport: None,
            hid_serial: String::new(),
            hid_error: String::new(),
            packets_received: 0,
            last_packet_at: None,
            started_at: Instant::now(),
            udp_bound: false,
            web_url: String::new(),
            last_settings_save_error: String::new(),
            update_status: UpdateStatus::default(),
            last_trigger_input: None,
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

/// JSON snapshot served over HTTP/WebSocket. We hand-roll this rather
/// than blanket-deriving Serialize on AppState so the wire format stays
/// stable across refactors.
#[derive(Serialize)]
pub struct StateSnapshot<'a> {
    pub hid_status: HidStatus,
    pub hid_transport: Option<&'static str>,
    pub hid_serial: &'a str,
    pub hid_error: &'a str,
    pub udp_bound: bool,
    pub udp_port: u16,
    pub udp_host: &'a str,
    pub packets_received: u64,
    pub packets_per_sec: f32,
    pub seconds_since_packet: Option<f32>,
    pub telemetry: Telemetry,
    /// L2/R2 trigger positions for the live cursor on the curve graphs.
    /// Either the game's brake/accel when telemetry is active, or the
    /// controller's actual analog inputs when idle.
    pub live_l2: u8,
    pub live_r2: u8,
    pub uptime_s: f32,
    pub settings: &'a Settings,
    pub update_status: &'a UpdateStatus,
}

impl AppState {
    pub fn snapshot(&self, pps: f32) -> StateSnapshot<'_> {
        let (live_l2, live_r2) = if self.telemetry.on {
            (self.telemetry.brake, self.telemetry.accel)
        } else {
            self.last_trigger_input.unwrap_or((0, 0))
        };
        StateSnapshot {
            hid_status: self.hid_status,
            hid_transport: self.hid_transport.map(|t| match t {
                crate::hid::Transport::Usb => "usb",
                crate::hid::Transport::Bluetooth => "bluetooth",
            }),
            hid_serial: &self.hid_serial,
            hid_error: &self.hid_error,
            udp_bound: self.udp_bound,
            udp_port: self.settings.udp_port,
            udp_host: &self.settings.udp_host,
            packets_received: self.packets_received,
            packets_per_sec: pps,
            seconds_since_packet: self
                .last_packet_at
                .map(|t| t.elapsed().as_secs_f32()),
            telemetry: self.telemetry,
            live_l2,
            live_r2,
            uptime_s: self.started_at.elapsed().as_secs_f32(),
            settings: &self.settings,
            update_status: &self.update_status,
        }
    }
}
