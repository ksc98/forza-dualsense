//! Background thread: open the DualSense, run the trigger Controller on
//! a ~250 Hz cadence, and reconnect on disconnect.
//!
//! Runs on a dedicated OS thread (not a tokio task) because hidapi's
//! HidDevice is `!Send`. That's fine — the only state shared with the
//! rest of the app is `Arc<Mutex<AppState>>`.

use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::controller::Controller;
use crate::hid::DualSense;
use crate::state::{HidStatus, SharedState};
use crate::triggers::Effect;

const TICK_HZ: u64 = 250;

pub fn run(state: SharedState) {
    loop {
        let reconnect_s = state.lock().settings.reconnect_interval_s;
        match DualSense::open() {
            Ok(dev) => {
                {
                    let mut s = state.lock();
                    s.hid_status = HidStatus::Connected;
                    s.hid_transport = Some(dev.transport());
                    s.hid_serial = dev.serial.clone();
                    s.hid_error.clear();
                }
                if let Err(e) = drive(&dev, &state) {
                    let mut s = state.lock();
                    s.hid_status = HidStatus::Error;
                    s.hid_error = e.to_string();
                    tracing::warn!("HID loop exited: {e}");
                }
                let mut s = state.lock();
                s.hid_status = HidStatus::Disconnected;
                s.hid_transport = None;
                s.last_l2 = Effect::Off;
                s.last_r2 = Effect::Off;
            }
            Err(e) => {
                {
                    let mut s = state.lock();
                    s.hid_status = HidStatus::Disconnected;
                    s.hid_error = e.to_string();
                }
                tracing::debug!("DualSense not present: {e}");
            }
        }
        sleep(Duration::from_secs_f32(reconnect_s));
    }
}

fn drive(dev: &DualSense, state: &SharedState) -> anyhow::Result<()> {
    let mut controller = {
        let s = state.lock();
        Controller::new(&s.settings)
    };

    // Optional startup pulse so the user can feel the controller is alive.
    {
        let enable;
        let pulse_force;
        {
            let s = state.lock();
            enable = s.settings.enable_startup_pulse;
            pulse_force = s.settings.startup_pulse_force;
        }
        if enable {
            let pulse = Effect::rigid(pulse_force as f32);
            dev.write_triggers(&pulse, &pulse)?;
            sleep(Duration::from_millis(150));
            dev.write_triggers(&Effect::Off, &Effect::Off)?;
        }
    }

    let tick = Duration::from_micros(1_000_000 / TICK_HZ);
    let mut next_deadline = Instant::now();

    let mut last_telemetry_change = Instant::now();
    let mut last_seen_packets: u64 = 0;

    loop {
        next_deadline += tick;
        let now = Instant::now();
        if next_deadline > now {
            sleep(next_deadline - now);
        } else {
            // Fell behind — reset the deadline so we don't burn CPU
            // catching up.
            next_deadline = now;
        }

        let (telemetry, settings, packets, exit_on_close, telemetry_lost_s) = {
            let s = state.lock();
            (
                s.telemetry,
                s.settings.clone(),
                s.packets_received,
                s.settings.exit_on_game_close,
                s.settings.telemetry_lost_exit_s,
            )
        };

        if packets != last_seen_packets {
            last_seen_packets = packets;
            last_telemetry_change = Instant::now();
        } else if exit_on_close
            && packets > 0
            && last_telemetry_change.elapsed() > Duration::from_secs_f32(telemetry_lost_s)
        {
            // Telemetry has been silent long enough — neutral triggers
            // and idle. The supervisor (this same fn) keeps spinning.
            dev.write_triggers(&Effect::Off, &Effect::Off)?;
        }

        let (l2, r2) = controller.update(&telemetry, &settings);
        dev.write_triggers(&l2, &r2)?;
        {
            let mut s = state.lock();
            s.last_l2 = l2;
            s.last_r2 = r2;
        }
    }
}
