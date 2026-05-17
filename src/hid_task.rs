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
const RECONNECT_INTERVAL: Duration = Duration::from_secs(10);
const TELEMETRY_LOST_AFTER: Duration = Duration::from_secs(60);

pub fn run(state: SharedState) {
    loop {
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
        sleep(RECONNECT_INTERVAL);
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
            dev.write_triggers(&pulse, &pulse, (0, 0, 0))?;
            sleep(Duration::from_millis(150));
            dev.write_triggers(&Effect::Off, &Effect::Off, (0, 0, 0))?;
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

        // Pull the freshest HID input report (non-blocking). When the
        // game is dead, idle preview uses these analog values so the
        // user feels their own physical press through the curves.
        let idle_press = dev.read_inputs();
        if let Some(p) = idle_press {
            state.lock().last_trigger_input = Some(p);
        }

        let (telemetry, settings, packets) = {
            let s = state.lock();
            (s.telemetry, s.settings.clone(), s.packets_received)
        };

        // Track the peak press the user has ever produced, from either
        // source. Drives the calibration helper in the GUI so the wall
        // can be set right at the user's real max instead of an
        // assumed 255.
        {
            let (l2_press, r2_press) = if telemetry.on {
                (telemetry.brake, telemetry.accel)
            } else {
                idle_press.unwrap_or((0, 0))
            };
            let mut s = state.lock();
            s.max_l2_seen = s.max_l2_seen.max(l2_press);
            s.max_r2_seen = s.max_r2_seen.max(r2_press);
        }

        if packets != last_seen_packets {
            last_seen_packets = packets;
            last_telemetry_change = Instant::now();
        } else if packets > 0 && last_telemetry_change.elapsed() > TELEMETRY_LOST_AFTER {
            // Telemetry has been silent long enough — neutral triggers
            // and idle. The supervisor (this same fn) keeps spinning.
            dev.write_triggers(&Effect::Off, &Effect::Off, (0, 0, 0))?;
        }

        let (l2, r2) = controller.update(&telemetry, &settings, idle_press);
        let lightbar = controller.lightbar(&telemetry, &settings);
        dev.write_triggers(&l2, &r2, lightbar)?;
    }
}
