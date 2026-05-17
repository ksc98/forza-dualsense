use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use serde::Serialize;

use crate::settings::Settings;
use crate::telemetry::Telemetry;
use crate::triggers::Effect;
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
    pub last_l2: Effect,
    pub last_r2: Effect,
    pub packets_received: u64,
    pub last_packet_at: Option<Instant>,
    pub started_at: Instant,
    pub udp_bound: bool,
    pub web_url: String,
    pub last_settings_save_error: String,
    pub update_status: UpdateStatus,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            telemetry: Telemetry::default(),
            hid_status: HidStatus::Disconnected,
            hid_transport: None,
            hid_serial: String::new(),
            hid_error: String::new(),
            last_l2: Effect::Off,
            last_r2: Effect::Off,
            packets_received: 0,
            last_packet_at: None,
            started_at: Instant::now(),
            udp_bound: false,
            web_url: String::new(),
            last_settings_save_error: String::new(),
            update_status: UpdateStatus::default(),
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
    pub l2: Effect,
    pub r2: Effect,
    pub uptime_s: f32,
    pub settings: &'a Settings,
    pub update_status: &'a UpdateStatus,
}

impl AppState {
    pub fn snapshot(&self, pps: f32) -> StateSnapshot<'_> {
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
            l2: self.last_l2,
            r2: self.last_r2,
            uptime_s: self.started_at.elapsed().as_secs_f32(),
            settings: &self.settings,
            update_status: &self.update_status,
        }
    }
}
