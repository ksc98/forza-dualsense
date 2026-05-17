use eframe::egui::{self, Color32, RichText, Stroke};
use egui_plot::{Line, Plot, PlotPoints, VLine};

use crate::settings::Settings;
use crate::state::{HidStatus, HistorySample, SharedState};
use crate::triggers::Effect;
use crate::update::Status as UpdateStatus;

pub struct GuiApp {
    state: SharedState,
}

impl GuiApp {
    pub fn new(state: SharedState, cc: &eframe::CreationContext<'_>) -> Self {
        apply_style(&cc.egui_ctx);
        Self { state }
    }
}

// Palette.
const ACCENT: Color32 = Color32::from_rgb(0, 180, 255);
const ACCENT_DIM: Color32 = Color32::from_rgb(0, 130, 190);
const DIM: Color32 = Color32::from_rgb(140, 145, 160);
const OK: Color32 = Color32::from_rgb(80, 220, 120);
const BAD: Color32 = Color32::from_rgb(255, 90, 90);
const WARN: Color32 = Color32::from_rgb(255, 180, 60);
const THROTTLE: Color32 = Color32::from_rgb(80, 220, 140);
const BRAKE: Color32 = Color32::from_rgb(255, 90, 100);
const PANEL_BG: Color32 = Color32::from_rgb(18, 22, 30);
const CARD_BG: Color32 = Color32::from_rgb(24, 28, 38);

fn apply_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.window_fill = PANEL_BG;
    v.panel_fill = PANEL_BG;
    v.extreme_bg_color = Color32::from_rgb(12, 15, 22);
    v.widgets.noninteractive.bg_fill = CARD_BG;
    v.widgets.inactive.bg_fill = Color32::from_rgb(36, 42, 54);
    v.widgets.hovered.bg_fill = Color32::from_rgb(48, 56, 72);
    v.widgets.active.bg_fill = Color32::from_rgb(54, 64, 82);
    v.selection.bg_fill = ACCENT_DIM.linear_multiply(0.7);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(20.0, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(33));

        let snapshot = self.collect_snapshot();

        egui::TopBottomPanel::top("hdr")
            .frame(egui::Frame::none().fill(PANEL_BG).inner_margin(egui::Margin::symmetric(14.0, 10.0)))
            .show(ctx, |ui| {
                header_bar(ui, &snapshot);
            });

        egui::SidePanel::right("settings_panel")
            .resizable(false)
            .exact_width(380.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Settings").size(18.0).strong());
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut new_settings = snapshot.settings.clone();
                        let mut changed = false;
                        changed |= settings_panel(ui, &mut new_settings);
                        if changed {
                            let mut s = self.state.lock();
                            s.settings = new_settings.clone();
                            drop(s);
                            let _ = new_settings.save();
                        }
                    });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(PANEL_BG).inner_margin(egui::Margin::symmetric(14.0, 12.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        update_banner(ui, &snapshot.update_status);
                        stat_strip(ui, &snapshot);
                        ui.add_space(10.0);
                        plots_section(ui, &snapshot);
                        ui.add_space(12.0);
                        triggers_section(ui, &snapshot);
                        ui.add_space(12.0);
                        diagnostics(ui, &snapshot);
                    });
            });
    }
}

impl GuiApp {
    fn collect_snapshot(&self) -> SnapshotForUi {
        let mut s = self.state.lock();

        // Push a new history sample at the GUI repaint rate (~30 Hz).
        let t = s.started_at.elapsed().as_secs_f32();
        let sample = HistorySample {
            t,
            speed_kmh: s.telemetry.speed_kmh,
            rpm: s.telemetry.rpm,
            max_rpm: s.telemetry.max_rpm,
            throttle: s.telemetry.accel as f32 / 255.0,
            brake: s.telemetry.brake as f32 / 255.0,
        };
        s.history.push(sample);

        let samples: Vec<HistorySample> = s.history.samples.iter().copied().collect();
        let now_t = t;

        SnapshotForUi {
            hid_status: s.hid_status,
            hid_transport: s.hid_transport.map(|t| match t {
                crate::hid::Transport::Usb => "USB",
                crate::hid::Transport::Bluetooth => "Bluetooth",
            }),
            hid_serial: s.hid_serial.clone(),
            hid_error: s.hid_error.clone(),
            udp_port: s.settings.udp_port,
            udp_host: s.settings.udp_host.clone(),
            packets: s.packets_received,
            seconds_since_packet: s.last_packet_at.map(|t| t.elapsed().as_secs_f32()),
            telemetry: s.telemetry,
            l2: s.last_l2,
            r2: s.last_r2,
            web_url: s.web_url.clone(),
            settings: s.settings.clone(),
            update_status: s.update_status.clone(),
            samples,
            now_t,
        }
    }
}

struct SnapshotForUi {
    hid_status: HidStatus,
    hid_transport: Option<&'static str>,
    hid_serial: String,
    hid_error: String,
    udp_port: u16,
    udp_host: String,
    packets: u64,
    seconds_since_packet: Option<f32>,
    telemetry: crate::telemetry::Telemetry,
    l2: Effect,
    r2: Effect,
    web_url: String,
    settings: Settings,
    update_status: UpdateStatus,
    samples: Vec<HistorySample>,
    now_t: f32,
}

// ────────────────────────────────────────────────────────────────────
// Top bar
// ────────────────────────────────────────────────────────────────────

fn header_bar(ui: &mut egui::Ui, snap: &SnapshotForUi) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Forza DualSense").size(22.0).strong().color(ACCENT));
        ui.label(
            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .color(DIM)
                .small(),
        );
        ui.add_space(16.0);

        status_dot(
            ui,
            "HID",
            match snap.hid_status {
                HidStatus::Connected => DotState::Ok,
                HidStatus::Disconnected => DotState::Pending,
                HidStatus::Error => DotState::Bad,
            },
            match snap.hid_status {
                HidStatus::Connected => snap.hid_transport.unwrap_or("connected").to_string(),
                HidStatus::Disconnected => "waiting".into(),
                HidStatus::Error => "error".into(),
            },
        );

        let udp_alive = snap.seconds_since_packet.map(|s| s < 2.0).unwrap_or(false);
        status_dot(
            ui,
            "UDP",
            if udp_alive { DotState::Ok } else { DotState::Pending },
            format!("{}:{}", snap.udp_host, snap.udp_port),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !snap.web_url.is_empty() {
                ui.hyperlink_to(
                    RichText::new(format!("Web UI  ↗  {}", snap.web_url)).color(ACCENT),
                    &snap.web_url,
                );
            }
        });
    });
}

#[derive(Clone, Copy)]
enum DotState { Ok, Pending, Bad }

fn status_dot(ui: &mut egui::Ui, label: &str, state: DotState, value: String) {
    let color = match state {
        DotState::Ok => OK,
        DotState::Pending => WARN,
        DotState::Bad => BAD,
    };
    ui.label(RichText::new("●").color(color).size(12.0));
    ui.label(RichText::new(label).strong().small());
    ui.label(RichText::new(value).color(DIM).small());
    ui.add_space(10.0);
}

// ────────────────────────────────────────────────────────────────────
// Quick-glance stat strip
// ────────────────────────────────────────────────────────────────────

fn stat_strip(ui: &mut egui::Ui, snap: &SnapshotForUi) {
    let t = &snap.telemetry;
    ui.horizontal(|ui| {
        stat_card(ui, "SPEED", format!("{:>5.0} km/h", t.speed_kmh), ACCENT);
        stat_card(ui, "RPM", format!("{:>5.0}", t.rpm), if t.max_rpm > 0.0 && t.rpm / t.max_rpm > 0.93 { BAD } else { WARN });
        stat_card(ui, "GEAR", gear_label(t.gear), Color32::from_rgb(220, 220, 230));
        stat_card(
            ui,
            "RACE",
            if t.on { "live".into() } else { "idle".into() },
            if t.on { OK } else { DIM },
        );
    });
}

fn gear_label(g: u8) -> String {
    match g {
        0 => "N".into(),
        n => format!("{n}"),
    }
}

fn stat_card(ui: &mut egui::Ui, label: &str, value: String, value_color: Color32) {
    egui::Frame::none()
        .fill(CARD_BG)
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(14.0, 8.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(label).small().color(DIM).strong());
                ui.label(RichText::new(value).size(18.0).color(value_color).strong().monospace());
            });
        });
}

// ────────────────────────────────────────────────────────────────────
// Live plots
// ────────────────────────────────────────────────────────────────────

const PLOT_WINDOW_S: f32 = 20.0;

fn plots_section(ui: &mut egui::Ui, snap: &SnapshotForUi) {
    let samples = &snap.samples;
    let now = snap.now_t;
    let window_start = now - PLOT_WINDOW_S;

    // Build series. X axis is "seconds from now" (negative = past, 0 = now).
    let rel = |s: &HistorySample| (s.t - now) as f64;

    let speed: PlotPoints = samples
        .iter()
        .filter(|s| s.t >= window_start)
        .map(|s| [rel(s), s.speed_kmh as f64])
        .collect();
    let rpm: PlotPoints = samples
        .iter()
        .filter(|s| s.t >= window_start)
        .map(|s| [rel(s), s.rpm as f64])
        .collect();
    let throttle: PlotPoints = samples
        .iter()
        .filter(|s| s.t >= window_start)
        .map(|s| [rel(s), s.throttle as f64])
        .collect();
    let brake: PlotPoints = samples
        .iter()
        .filter(|s| s.t >= window_start)
        .map(|s| [rel(s), s.brake as f64])
        .collect();

    let max_rpm = samples.iter().map(|s| s.max_rpm).fold(0.0_f32, f32::max).max(8000.0);

    plot_card(ui, "Speed (km/h)", |ui| {
        Plot::new("plot_speed")
            .height(90.0)
            .show_axes([false, true])
            .show_grid([false, true])
            .include_x(-(PLOT_WINDOW_S as f64))
            .include_x(0.0)
            .include_y(0.0)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show(ui, |p| {
                p.line(Line::new(speed).color(ACCENT).width(2.0));
                p.vline(VLine::new(0.0).color(Color32::from_rgba_premultiplied(80, 90, 110, 100)));
            });
    });

    plot_card(ui, "RPM", |ui| {
        Plot::new("plot_rpm")
            .height(90.0)
            .show_axes([false, true])
            .show_grid([false, true])
            .include_x(-(PLOT_WINDOW_S as f64))
            .include_x(0.0)
            .include_y(0.0)
            .include_y(max_rpm as f64)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show(ui, |p| {
                p.line(Line::new(rpm).color(WARN).width(2.0));
                p.vline(VLine::new(0.0).color(Color32::from_rgba_premultiplied(80, 90, 110, 100)));
            });
    });

    plot_card(ui, "Inputs (throttle / brake)", |ui| {
        Plot::new("plot_inputs")
            .height(90.0)
            .show_axes([false, true])
            .show_grid([false, true])
            .include_x(-(PLOT_WINDOW_S as f64))
            .include_x(0.0)
            .include_y(0.0)
            .include_y(1.0)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show(ui, |p| {
                p.line(Line::new(throttle).color(THROTTLE).width(2.0).name("throttle"));
                p.line(Line::new(brake).color(BRAKE).width(2.0).name("brake"));
                p.vline(VLine::new(0.0).color(Color32::from_rgba_premultiplied(80, 90, 110, 100)));
            });
    });
}

fn plot_card(ui: &mut egui::Ui, label: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(CARD_BG)
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.label(RichText::new(label).small().strong().color(DIM));
            ui.add_space(2.0);
            body(ui);
        });
}

// ────────────────────────────────────────────────────────────────────
// Trigger effect cards
// ────────────────────────────────────────────────────────────────────

fn triggers_section(ui: &mut egui::Ui, snap: &SnapshotForUi) {
    ui.label(RichText::new("Trigger effects").size(16.0).strong());
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        effect_card(ui, "L2  ·  BRAKE", &snap.l2, BRAKE);
        effect_card(ui, "R2  ·  THROTTLE", &snap.r2, THROTTLE);
    });
}

fn effect_card(ui: &mut egui::Ui, title: &str, eff: &Effect, accent: Color32) {
    egui::Frame::none()
        .fill(CARD_BG)
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.set_min_width(280.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).small().strong().color(DIM));
                ui.add_space(4.0);
                let (mode, detail) = describe_effect(eff);
                ui.label(RichText::new(mode).size(15.0).color(accent).strong().monospace());
                if !detail.is_empty() {
                    ui.label(RichText::new(detail).color(DIM).monospace().small());
                }
                ui.add_space(4.0);
                ui.add(
                    egui::ProgressBar::new(eff.display_force())
                        .desired_height(8.0)
                        .fill(accent),
                );
            });
        });
}

fn describe_effect(eff: &Effect) -> (String, String) {
    match *eff {
        Effect::Off => ("OFF".into(), String::new()),
        Effect::Rigid { force } => ("RIGID".into(), format!("force {force}")),
        Effect::Vibration { freq, amp } => ("VIBRATION".into(), format!("{freq} Hz · amp {amp}")),
        Effect::VibrationWall { freq, amp_strength, wall_zones } => (
            "WALL PULSE".into(),
            format!("{freq} Hz · amp {amp_strength} · zones {wall_zones}"),
        ),
        Effect::Feedback { .. } => ("WALL".into(), String::new()),
    }
}

// ────────────────────────────────────────────────────────────────────
// Update banner & diagnostics
// ────────────────────────────────────────────────────────────────────

fn update_banner(ui: &mut egui::Ui, status: &UpdateStatus) {
    match status {
        UpdateStatus::Applied { version } => {
            egui::Frame::none()
                .fill(Color32::from_rgb(20, 60, 40))
                .rounding(8.0)
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "Update {version} downloaded. Restart to apply."
                        ))
                        .color(OK)
                        .strong(),
                    );
                });
            ui.add_space(8.0);
        }
        UpdateStatus::Failed { error } => {
            ui.label(
                RichText::new(format!("Update check failed: {error}"))
                    .color(DIM)
                    .small(),
            );
            ui.add_space(4.0);
        }
        _ => {}
    }
}

fn diagnostics(ui: &mut egui::Ui, snap: &SnapshotForUi) {
    ui.collapsing(RichText::new("Diagnostics").color(DIM), |ui| {
        ui.label(format!("Packets received: {}", snap.packets));
        if let Some(s) = snap.seconds_since_packet {
            ui.label(format!("Last packet: {:.1}s ago", s));
        } else {
            ui.label("No packet received yet");
        }
        if !snap.hid_error.is_empty() {
            ui.label(RichText::new(format!("HID: {}", snap.hid_error)).color(BAD));
        }
        if !snap.hid_serial.is_empty() {
            ui.label(format!("Controller serial: {}", snap.hid_serial));
        }
        ui.label(format!(
            "Slip — ratio {:.2} · combined {:.2}",
            snap.telemetry.max_slip_ratio(),
            snap.telemetry.max_combined_slip()
        ));
    });
}

// ────────────────────────────────────────────────────────────────────
// Settings panel
// ────────────────────────────────────────────────────────────────────

fn settings_panel(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    let mut changed = false;
    changed |= section_brake(ui, s);
    changed |= section_abs(ui, s);
    changed |= section_throttle(ui, s);
    changed |= section_shift_rev(ui, s);
    changed |= section_system(ui, s);
    changed
}

fn header(ui: &mut egui::Ui, label: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(label).strong().color(ACCENT));
    ui.separator();
}

fn slider_u8(ui: &mut egui::Ui, label: &str, v: &mut u8, lo: u8, hi: u8) -> bool {
    let mut tmp = *v as i32;
    let r = ui.add(egui::Slider::new(&mut tmp, lo as i32..=hi as i32).text(label));
    if r.changed() {
        *v = tmp as u8;
        true
    } else {
        false
    }
}

fn slider_f32(ui: &mut egui::Ui, label: &str, v: &mut f32, lo: f32, hi: f32) -> bool {
    ui.add(egui::Slider::new(v, lo..=hi).text(label)).changed()
}

fn section_brake(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    let mut c = false;
    header(ui, "Brake (L2)");
    c |= ui.checkbox(&mut s.enable_brake_resistance, "Resistance").changed();
    c |= slider_u8(ui, "Deadzone", &mut s.brake_deadzone, 0, 255);
    c |= slider_u8(ui, "Baseline force", &mut s.brake_baseline_force, 0, 255);
    c |= slider_u8(ui, "Max force", &mut s.brake_max_force, 0, 255);
    c |= slider_f32(ui, "Curve", &mut s.brake_curve, 0.5, 8.0);
    c |= slider_u8(ui, "Wall engage at", &mut s.brake_wall_engage_at, 0, 255);
    c |= slider_u8(ui, "Wall release at", &mut s.brake_wall_release_at, 0, 255);
    c |= ui.checkbox(&mut s.enable_handbrake_bonus, "Handbrake bonus").changed();
    c |= slider_u8(ui, "Handbrake bonus force", &mut s.handbrake_bonus, 0, 255);
    c
}

fn section_abs(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    let mut c = false;
    header(ui, "ABS / slip pulse");
    c |= ui.checkbox(&mut s.enable_abs, "Enable").changed();
    c |= slider_u8(ui, "Brake threshold", &mut s.abs_brake_threshold, 0, 255);
    c |= slider_f32(ui, "Min speed (km/h)", &mut s.abs_min_speed_kmh, 0.0, 80.0);
    c |= slider_f32(ui, "Slip ratio threshold", &mut s.abs_slip_ratio_threshold, 0.0, 3.0);
    c |= slider_f32(
        ui,
        "Combined slip threshold",
        &mut s.abs_combined_slip_threshold,
        0.0,
        3.0,
    );
    c |= slider_u8(ui, "Freq (Hz)", &mut s.abs_freq, 1, 60);
    c |= slider_u8(ui, "Amp", &mut s.abs_amp, 0, 255);
    c
}

fn section_throttle(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    let mut c = false;
    header(ui, "Throttle (R2)");
    c |= ui.checkbox(&mut s.enable_throttle_resistance, "Resistance").changed();
    c |= slider_u8(ui, "Deadzone", &mut s.accel_deadzone, 0, 255);
    c |= slider_u8(ui, "Baseline force", &mut s.throttle_baseline_force, 0, 255);
    c |= slider_u8(ui, "Max force", &mut s.throttle_max_force, 0, 255);
    c |= slider_f32(ui, "Curve", &mut s.throttle_curve, 0.5, 8.0);
    c |= slider_u8(ui, "Wall engage at", &mut s.throttle_wall_engage_at, 0, 255);
    c |= slider_u8(ui, "Wall release at", &mut s.throttle_wall_release_at, 0, 255);
    c
}

fn section_shift_rev(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    let mut c = false;
    header(ui, "Gear shift");
    c |= ui.checkbox(&mut s.enable_gear_shift, "On throttle").changed();
    c |= ui.checkbox(&mut s.enable_gear_shift_brake, "On brake").changed();
    c |= slider_u8(ui, "Freq (Hz)", &mut s.gear_shift_freq, 1, 60);
    c |= slider_u8(ui, "Amp", &mut s.gear_shift_amp, 0, 255);
    c |= slider_f32(ui, "Duration (ms)", &mut s.gear_shift_duration_ms, 20.0, 400.0);

    header(ui, "Rev limiter");
    c |= ui.checkbox(&mut s.enable_rev_limiter, "Enable").changed();
    c |= slider_f32(ui, "Ratio (rpm/max)", &mut s.rev_limit_ratio, 0.5, 1.0);
    c |= slider_u8(ui, "Freq (Hz)", &mut s.rev_limit_freq, 1, 60);
    c |= slider_u8(ui, "Amp", &mut s.rev_limit_amp, 0, 255);
    c |= slider_f32(ui, "Hold (ms)", &mut s.rev_limit_hold_ms, 0.0, 500.0);
    c
}

fn section_system(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    let mut c = false;
    header(ui, "System");
    c |= ui.checkbox(&mut s.enable_startup_pulse, "Startup pulse").changed();
    c |= slider_u8(ui, "Pulse force", &mut s.startup_pulse_force, 0, 255);
    c |= slider_u8(ui, "Wall zones", &mut s.wall_zones, 1, 9);
    c |= ui
        .checkbox(&mut s.enable_auto_update, "Check for updates on launch")
        .changed();

    header(ui, "Debug");
    c |= ui
        .checkbox(
            &mut s.enable_test_force,
            "Drive triggers from sliders when game is not streaming",
        )
        .changed();
    c |= slider_u8(ui, "Simulated brake", &mut s.test_brake, 0, 255);
    c |= slider_u8(ui, "Simulated throttle", &mut s.test_throttle, 0, 255);
    ui.label(
        RichText::new(
            "Feeds these inputs through the same brake and throttle force curves the game uses, so you feel the configured resistance without launching Forza.",
        )
        .color(DIM)
        .small(),
    );

    ui.add_space(6.0);
    ui.label(RichText::new(format!("UDP {}:{}", s.udp_host, s.udp_port)).color(DIM).small());
    ui.label(RichText::new("UDP address fixed for the session — restart to change.").color(DIM).small());
    c
}
