//! DualSense HID output report.
//!
//! Layout matches Sony's public DualSense report format. We only ever
//! set the trigger bits in the valid-flag byte so Steam Input keeps
//! owning the rumble motors.

use anyhow::{anyhow, Context, Result};
use hidapi::{HidApi, HidDevice};

use crate::triggers::Effect;

pub const VID_SONY: u16 = 0x054C;
pub const PID_DUALSENSE: u16 = 0x0CE6;
pub const PID_DUALSENSE_EDGE: u16 = 0x0DF2;

/// Bit 2 = R trigger effect, bit 3 = L trigger effect. Leaving bits 0/1
/// cleared keeps Steam's rumble bytes untouched. When we also want to
/// drive the rumble motors (redline buzz) we OR in `FLAGS_RUMBLE` so the
/// firmware reads our motor bytes.
const FLAGS_TRIGGERS_ONLY: u8 = 0x04 | 0x08;
/// bit 0 = enable compat rumble (motor bytes), bit 1 = "use rumble not
/// haptics" — together they make the rumble bytes authoritative.
const FLAGS_RUMBLE: u8 = 0x01 | 0x02;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Usb,
    Bluetooth,
}

impl Transport {
    pub fn report_size(self) -> usize {
        match self {
            Transport::Usb => 64,
            Transport::Bluetooth => 78,
        }
    }

    pub fn report_id(self) -> u8 {
        match self {
            Transport::Usb => 0x02,
            Transport::Bluetooth => 0x31,
        }
    }

    /// Byte offset (inside the buffer we hand to hidapi, which already
    /// includes a leading report-id byte) of the flags byte.
    fn flags_off(self) -> usize {
        match self {
            // [report_id, flags, valid_flag1, ...]
            Transport::Usb => 1,
            // [report_id, ???, flags, valid_flag1, ...]
            Transport::Bluetooth => 2,
        }
    }

    fn right_trigger_off(self) -> usize {
        match self {
            Transport::Usb => 11,
            Transport::Bluetooth => 12,
        }
    }

    fn left_trigger_off(self) -> usize {
        match self {
            Transport::Usb => 22,
            Transport::Bluetooth => 23,
        }
    }

    /// `(right_motor, left_motor)` byte offsets — right is the
    /// high-frequency (light) motor, left is the low-frequency (heavy)
    /// motor in the DualSense compat-rumble layout.
    fn rumble_off(self) -> (usize, usize) {
        match self {
            Transport::Usb => (3, 4),
            Transport::Bluetooth => (4, 5),
        }
    }
}

pub struct DualSense {
    device: HidDevice,
    transport: Transport,
    pub serial: String,
}

impl DualSense {
    pub fn open() -> Result<Self> {
        let api = HidApi::new().context("failed to init hidapi")?;
        for info in api.device_list() {
            if info.vendor_id() != VID_SONY {
                continue;
            }
            if !matches!(info.product_id(), PID_DUALSENSE | PID_DUALSENSE_EDGE) {
                continue;
            }
            let device = info
                .open_device(&api)
                .context("found DualSense but failed to open it (HidHide blocking? permissions?)")?;
            // Non-blocking writes — important on Bluetooth where the
            // device would otherwise stall waiting for an input report.
            device.set_blocking_mode(false).ok();

            // Heuristic: USB reports start with 0x01, Bluetooth with
            // 0x31. We probe with a short read; if it fails or returns 0
            // bytes we default to USB which is by far the common case.
            let transport = probe_transport(&device);

            let serial = info.serial_number().unwrap_or("").to_string();
            return Ok(Self { device, transport, serial });
        }
        Err(anyhow!("DualSense gamepad interface not found"))
    }

    pub fn transport(&self) -> Transport {
        self.transport
    }

    /// Latest analog `(L2, R2)` press from the controller's HID input
    /// reports, or `None` if no report has arrived since the last call.
    /// Drains every queued report (controllers stream at ~250 Hz; if
    /// the GUI thread stalls briefly, reports back up in the kernel
    /// buffer) and returns the freshest one so the live cursor never
    /// shows a stale value.
    ///
    /// Layouts:
    ///   USB report 0x01: byte 5 = L2, byte 6 = R2.
    ///   BT  report 0x31: same payload prefixed with `[id, seq]`, so
    ///                    byte 6 = L2, byte 7 = R2.
    pub fn read_inputs(&self) -> Option<(u8, u8)> {
        let mut buf = [0u8; 78];
        let mut latest = None;
        loop {
            match self.device.read_timeout(&mut buf, 0) {
                Ok(n) if n > 0 => {
                    let parsed = match self.transport {
                        Transport::Usb if n >= 7 && buf[0] == 0x01 => Some((buf[5], buf[6])),
                        Transport::Bluetooth if n >= 8 && buf[0] == 0x31 => {
                            Some((buf[6], buf[7]))
                        }
                        _ => None,
                    };
                    if parsed.is_some() {
                        latest = parsed;
                    }
                }
                _ => return latest,
            }
        }
    }

    pub fn write_triggers(&self, l2: &Effect, r2: &Effect) -> Result<()> {
        self.write_outputs(l2, r2, 0, 0)
    }

    /// Same as `write_triggers` but also drives the main rumble motors.
    /// `rumble_heavy` is the low-frequency (left) motor; `rumble_light`
    /// is the high-frequency (right) motor. Pass `0, 0` to leave the
    /// motors alone and keep Steam Input's rumble passthrough working.
    pub fn write_outputs(
        &self,
        l2: &Effect,
        r2: &Effect,
        rumble_heavy: u8,
        rumble_light: u8,
    ) -> Result<()> {
        let size = self.transport.report_size();
        let mut buf = vec![0u8; size];
        buf[0] = self.transport.report_id();
        // BT report 0x31: byte 1 is the sequence/tag byte. The zero-init
        // above leaves it at 0x00, which every DualSense firmware seen
        // in the wild tolerates. If a future firmware enforces Sony's
        // rotating-nibble protocol, add a per-write counter here.
        let mut flags = FLAGS_TRIGGERS_ONLY;
        if rumble_heavy != 0 || rumble_light != 0 {
            flags |= FLAGS_RUMBLE;
            let (r_rumble, l_rumble) = self.transport.rumble_off();
            buf[r_rumble] = rumble_light;
            buf[l_rumble] = rumble_heavy;
        }
        buf[self.transport.flags_off()] = flags;

        let r_off = self.transport.right_trigger_off();
        let l_off = self.transport.left_trigger_off();

        let (rmode, rparams) = r2.to_hid_bytes();
        buf[r_off] = rmode;
        buf[r_off + 1..r_off + 11].copy_from_slice(&rparams);

        let (lmode, lparams) = l2.to_hid_bytes();
        buf[l_off] = lmode;
        buf[l_off + 1..l_off + 11].copy_from_slice(&lparams);

        if self.transport == Transport::Bluetooth {
            // CRC32 over [0xA2] || buf[0..74], stored at bytes 74..78.
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(&[0xA2]);
            hasher.update(&buf[0..74]);
            let crc = hasher.finalize().to_le_bytes();
            buf[74..78].copy_from_slice(&crc);
        }

        self.device.write(&buf).context("HID write failed")?;
        Ok(())
    }
}

fn probe_transport(device: &HidDevice) -> Transport {
    let mut buf = [0u8; 78];
    match device.read_timeout(&mut buf, 50) {
        Ok(n) if n > 0 => match buf[0] {
            0x31 => Transport::Bluetooth,
            _ => Transport::Usb,
        },
        _ => Transport::Usb,
    }
}
