use eframe::egui::{self, Color32, RichText, Stroke};
#[cfg(windows)]
use eframe::egui::ViewportCommand;
#[cfg(windows)]
use tray_icon::{menu::MenuEvent, TrayIconEvent};

use crate::settings::Settings;
use crate::state::{HidStatus, SharedState};
#[cfg(windows)]
use crate::tray::Tray;
use crate::update::Status as UpdateStatus;

pub struct GuiApp {
    state: SharedState,
    #[cfg(windows)]
    tray: Option<Tray>,
    /// Settings the user is currently dragging — kept out of the shared
    /// state until the debounce timer expires, so a slider drag doesn't
    /// fsync the config file on every frame.
    pending_save: Option<(Settings, std::time::Instant)>,
    /// True iff we've hidden the window to the tray. We only act on the
    /// minimize→hide transition once, otherwise restoring via the tray
    /// keeps re-hiding the window because `viewport().minimized` is
    /// still true for a frame or two after `Minimized(false)` is sent.
    #[cfg(windows)]
    hidden_to_tray: bool,
}

const SAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(400);

impl GuiApp {
    pub fn new(state: SharedState, cc: &eframe::CreationContext<'_>) -> Self {
        apply_style(&cc.egui_ctx);
        #[cfg(windows)]
        let tray = match Tray::build() {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::warn!("Tray icon unavailable: {e}");
                None
            }
        };
        Self {
            state,
            #[cfg(windows)]
            tray,
            pending_save: None,
            #[cfg(windows)]
            hidden_to_tray: false,
        }
    }
}

#[cfg(windows)]
fn restore_window(ctx: &egui::Context, hidden_to_tray: &mut bool) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
    // Clear our state machine *before* the next minimize-to-tray check
    // runs — otherwise the lingering viewport().minimized=true reading
    // would immediately re-hide us.
    *hidden_to_tray = false;
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

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(20.0, egui::FontFamily::Proportional),
    );
    ctx.set_global_style(style);
}

impl eframe::App for GuiApp {
    /// eframe persists egui memory (including SidePanel widths) to disk
    /// by default. That meant earlier versions where the settings panel
    /// was resizable could leave behind a tiny width that stuck around
    /// across upgrades and overrode `exact_width()`. We don't rely on
    /// any persisted UI state, so just turn it off.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        // 120 Hz redraw so the live trigger cursor on the curve graphs
        // tracks the player's press without visible lag. The HID worker
        // already polls the controller at 250 Hz, so we're not making
        // up data — just rendering it as fast as the display can take.
        ctx.request_repaint_after(std::time::Duration::from_millis(8));

        #[cfg(windows)]
        if let Some(tray) = &self.tray {
            // Drain tray-icon events. Tray callbacks fire on the
            // message-pump thread; we read from the global channels
            // each frame.
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                if ev.id == tray.show_id {
                    restore_window(ctx, &mut self.hidden_to_tray);
                } else if ev.id == tray.quit_id {
                    std::process::exit(0);
                }
            }
            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                // Restore on left-click release. We key on `Up` so a
                // single click registers exactly once, not on both
                // press and release.
                if let TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                } = ev
                {
                    restore_window(ctx, &mut self.hidden_to_tray);
                }
            }
            // Minimize -> hide to tray. Only act on the false→true
            // transition: if we react to the steady "minimized" state we
            // re-hide the window the frame after the user clicks Show,
            // because viewport().minimized takes a frame to clear.
            let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
            if minimized && !self.hidden_to_tray {
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                self.hidden_to_tray = true;
            }
        }

        let snapshot = self.collect_snapshot();

        egui::Panel::top("hdr")
            .frame(egui::Frame::new().fill(PANEL_BG).inner_margin(egui::Margin::symmetric(14, 10)))
            .show_inside(ui, |ui| {
                header_bar(ui, &snapshot);
            });

        egui::Panel::left("settings_panel_v4_left")
            .resizable(false)
            .exact_size(380.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("settings_scroll")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Settings").size(18.0).strong());
                        ui.separator();
                        let mut new_settings = snapshot.settings.clone();
                        let mut changed = false;
                        changed |= settings_panel(
                            ui,
                            &mut new_settings,
                            snapshot.max_l2_seen,
                            snapshot.max_r2_seen,
                        );
                        if changed {
                            self.state.lock().settings = new_settings.clone();
                            self.pending_save = Some((new_settings, std::time::Instant::now()));
                        }
                    });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(PANEL_BG).inner_margin(egui::Margin::symmetric(14, 12)))
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("central_scroll")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        update_banner(ui, &snapshot.update_status);
                        stat_strip(ui, &snapshot);
                        ui.add_space(12.0);
                        curves_section(ui, &snapshot);
                        ui.add_space(12.0);
                        diagnostics(ui, &snapshot);
                    });
            });

        if let Some((_, t)) = &self.pending_save {
            if t.elapsed() >= SAVE_DEBOUNCE {
                let (settings, _) = self.pending_save.take().unwrap();
                std::thread::spawn(move || {
                    if let Err(e) = settings.save() {
                        tracing::warn!("settings save failed: {e}");
                    }
                });
            }
        }
    }
}

impl GuiApp {
    fn collect_snapshot(&self) -> SnapshotForUi {
        let s = self.state.lock();
        let logs = s.logs.0.lock().snapshot();

        // Curve cursor sits at the game's reported press when telemetry
        // is alive, otherwise the controller's actual L2/R2 if we have
        // one cached. Falls back to 0 so the graph still draws cleanly.
        let live = if s.telemetry.on {
            (s.telemetry.brake, s.telemetry.accel)
        } else {
            s.last_trigger_input.unwrap_or((0, 0))
        };

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
            live_l2: live.0,
            live_r2: live.1,
            max_l2_seen: s.max_l2_seen,
            max_r2_seen: s.max_r2_seen,
            web_url: s.web_url.clone(),
            settings: s.settings.clone(),
            update_status: s.update_status.clone(),
            logs,
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
    /// L2/R2 trigger positions to display on the live cursor. Either
    /// the game's reported brake/accel, or the controller's analog
    /// inputs when no telemetry is arriving.
    live_l2: u8,
    live_r2: u8,
    /// Peak L2 / R2 ever observed this session — drives the "Use peak
    /// as wall" calibration button under each pedal section.
    max_l2_seen: u8,
    max_r2_seen: u8,
    web_url: String,
    settings: Settings,
    update_status: UpdateStatus,
    logs: Vec<String>,
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
    egui::Frame::new()
        .fill(CARD_BG)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(label).small().color(DIM).strong());
                ui.label(RichText::new(value).size(18.0).color(value_color).strong().monospace());
            });
        });
}

// ────────────────────────────────────────────────────────────────────
// Live force-curve graphs
// ────────────────────────────────────────────────────────────────────
//
// Force-vs-trigger-position plots. Shows the curve the user has tuned
// up alongside their live trigger position. Replaces the old "L2/R2
// effect" cards which only told you the abstract mode — these directly
// show what configuration changes are doing, and where on the curve
// you currently are.

fn curves_section(ui: &mut egui::Ui, snap: &SnapshotForUi) {
    ui.label(RichText::new("Force curves").size(16.0).strong());
    ui.add_space(4.0);
    let total_w = ui.available_width();
    let card_w = ((total_w - 12.0) * 0.5).max(280.0);
    ui.horizontal(|ui| {
        curve_card(
            ui,
            "L2  ·  BRAKE",
            BRAKE,
            card_w,
            snap.live_l2,
            |v| crate::controller::brake_ramp(v, &snap.settings),
            &brake_markers(&snap.settings),
        );
        curve_card(
            ui,
            "R2  ·  THROTTLE",
            THROTTLE,
            card_w,
            snap.live_r2,
            |v| crate::controller::throttle_force(v, &snap.settings),
            &throttle_markers(&snap.settings),
        );
    });
}

struct CurveMarker {
    /// Trigger-position the marker sits at (0..255).
    x: u8,
    /// Short label drawn near the line.
    label: &'static str,
}

fn brake_markers(s: &Settings) -> Vec<CurveMarker> {
    vec![
        CurveMarker { x: s.brake_deadzone, label: "deadzone" },
        CurveMarker { x: s.brake_wall_engage_at, label: "wall" },
    ]
}

fn throttle_markers(s: &Settings) -> Vec<CurveMarker> {
    vec![
        CurveMarker { x: s.accel_deadzone, label: "deadzone" },
        CurveMarker { x: s.throttle_wall_engage_at, label: "wall" },
    ]
}

fn curve_card(
    ui: &mut egui::Ui,
    title: &str,
    accent: Color32,
    width: f32,
    live: u8,
    force_at: impl Fn(u8) -> f32,
    markers: &[CurveMarker],
) {
    use egui_plot::{Line, Plot, PlotPoints, VLine};

    egui::Frame::new()
        .fill(CARD_BG)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.label(RichText::new(title).small().strong().color(DIM));
            ui.add_space(2.0);

            // Sample the curve every 2 raw-pedal steps — fine enough that
            // the steep brake-bite segment doesn't visibly stair-step.
            let pts: PlotPoints = (0..=128u32)
                .map(|i| {
                    let x = (i * 2).min(255) as u8;
                    [x as f64, force_at(x) as f64]
                })
                .collect();

            let plot_id = format!("curve_{title}");
            let live_force = force_at(live);

            Plot::new(plot_id)
                .height(160.0)
                .show_axes([true, true])
                .show_grid([true, true])
                .include_x(0.0)
                .include_x(255.0)
                .include_y(0.0)
                .include_y(255.0)
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .show_x(false)
                .show_y(false)
                .show(ui, |p| {
                    for m in markers {
                        p.vline(
                            VLine::new(m.label, m.x as f64)
                                .color(Color32::from_rgba_premultiplied(120, 130, 150, 90)),
                        );
                    }
                    p.line(Line::new("force", pts).color(accent).width(2.0));
                    p.vline(VLine::new("pos", live as f64).color(OK).width(2.0));
                });

            ui.add_space(2.0);
            ui.label(
                RichText::new(format!("pos {live} → force {:.0}", live_force))
                    .color(DIM)
                    .monospace()
                    .small(),
            );
        });
}

// ────────────────────────────────────────────────────────────────────
// Update banner & diagnostics
// ────────────────────────────────────────────────────────────────────

fn update_banner(ui: &mut egui::Ui, status: &UpdateStatus) {
    match status {
        UpdateStatus::Applied { version } => {
            egui::Frame::new()
                .fill(Color32::from_rgb(20, 60, 40))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(12, 8))
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

    ui.collapsing(RichText::new("Logs").color(DIM), |ui| {
        egui::ScrollArea::vertical()
            .id_salt("log_scroll")
            .max_height(220.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let mono = egui::FontId::monospace(11.0);
                for line in &snap.logs {
                    ui.label(RichText::new(line).font(mono.clone()).color(DIM));
                }
            });
    });
}

// ────────────────────────────────────────────────────────────────────
// Settings panel
// ────────────────────────────────────────────────────────────────────

fn settings_panel(ui: &mut egui::Ui, s: &mut Settings, peak_l2: u8, peak_r2: u8) -> bool {
    let mut changed = false;
    changed |= section_brake(ui, s, peak_l2);
    changed |= section_abs(ui, s);
    changed |= section_throttle(ui, s, peak_r2);
    changed |= section_gear_shift(ui, s);
    changed |= section_lightbar(ui, s);
    changed |= section_system(ui, s);
    changed
}

/// Compact "Peak: N  [Use as wall]" row. Sets `wall_engage_at` to the
/// observed peak, keeping the existing hysteresis margin (min 20).
/// Disabled until the user has actually pressed the trigger.
fn calibration_row(
    ui: &mut egui::Ui,
    peak: u8,
    engage_at: &mut u8,
    release_at: &mut u8,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Peak: {peak}")).color(DIM).small());
        let button = egui::Button::new("Use as wall");
        let response = ui.add_enabled(peak > 0, button);
        if response.clicked() {
            let margin = engage_at.saturating_sub(*release_at).max(20);
            *engage_at = peak;
            *release_at = peak.saturating_sub(margin);
            changed = true;
        }
    });
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

fn shape_picker(ui: &mut egui::Ui, label: &str, value: &mut crate::settings::PedalShape) -> bool {
    use crate::settings::PedalShape;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let current = PedalShape::ALL
            .iter()
            .find(|(v, _)| v == value)
            .map(|(_, l)| *l)
            .unwrap_or("?");
        egui::ComboBox::from_id_salt(label)
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (variant, label) in PedalShape::ALL {
                    if ui
                        .selectable_label(*variant == *value, *label)
                        .clicked()
                    {
                        *value = *variant;
                        changed = true;
                    }
                }
            });
    });
    changed
}

fn section_brake(ui: &mut egui::Ui, s: &mut Settings, peak: u8) -> bool {
    let mut c = false;
    header(ui, "Brake (L2)");
    c |= ui.checkbox(&mut s.enable_brake_resistance, "Resistance").changed();
    c |= shape_picker(ui, "Shape", &mut s.brake_shape);
    c |= slider_u8(ui, "Min force", &mut s.brake_min_force, 0, 255);
    c |= slider_u8(ui, "Max force", &mut s.brake_max_force, 0, 255);
    c |= slider_u8(ui, "Deadzone", &mut s.brake_deadzone, 0, 255);
    c |= slider_u8(ui, "Wall engage at", &mut s.brake_wall_engage_at, 0, 255);
    c |= slider_u8(ui, "Wall release at", &mut s.brake_wall_release_at, 0, 255);
    c |= calibration_row(
        ui,
        peak,
        &mut s.brake_wall_engage_at,
        &mut s.brake_wall_release_at,
    );
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

fn section_throttle(ui: &mut egui::Ui, s: &mut Settings, peak: u8) -> bool {
    let mut c = false;
    header(ui, "Throttle (R2)");
    c |= ui.checkbox(&mut s.enable_throttle_resistance, "Resistance").changed();
    c |= shape_picker(ui, "Shape", &mut s.throttle_shape);
    c |= slider_u8(ui, "Min force", &mut s.throttle_min_force, 0, 255);
    c |= slider_u8(ui, "Max force", &mut s.throttle_max_force, 0, 255);
    c |= slider_u8(ui, "Deadzone", &mut s.accel_deadzone, 0, 255);
    c |= slider_u8(ui, "Wall engage at", &mut s.throttle_wall_engage_at, 0, 255);
    c |= slider_u8(ui, "Wall release at", &mut s.throttle_wall_release_at, 0, 255);
    c |= calibration_row(
        ui,
        peak,
        &mut s.throttle_wall_engage_at,
        &mut s.throttle_wall_release_at,
    );
    c
}

fn section_gear_shift(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    let mut c = false;
    header(ui, "Gear shift");
    c |= ui.checkbox(&mut s.enable_gear_shift, "On throttle").changed();
    c |= ui.checkbox(&mut s.enable_gear_shift_brake, "On brake").changed();
    c |= slider_u8(ui, "Freq (Hz)", &mut s.gear_shift_freq, 1, 60);
    c |= slider_u8(ui, "Amp", &mut s.gear_shift_amp, 0, 255);
    c |= slider_f32(ui, "Duration (ms)", &mut s.gear_shift_duration_ms, 20.0, 400.0);
    c
}

fn section_lightbar(ui: &mut egui::Ui, s: &mut Settings) -> bool {
    let mut c = false;
    header(ui, "Light bar");
    c |= ui
        .checkbox(&mut s.enable_lightbar, "Tachometer (green → red as RPM nears redline)")
        .changed();
    c |= slider_u8(ui, "Brightness", &mut s.lightbar_brightness, 0, 255);
    ui.label(
        RichText::new(
            "Overrides Steam Input's lightbar colour while enabled. Disable to give it back.",
        )
        .color(DIM)
        .small(),
    );
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

    header(ui, "Idle preview");
    c |= ui
        .checkbox(&mut s.enable_idle_preview, "Engage triggers when no game is running")
        .changed();
    ui.label(
        RichText::new(
            "Drives both triggers at a mid-pedal press through the configured force curves so you can feel changes without launching Forza.",
        )
        .color(DIM)
        .small(),
    );

    ui.add_space(6.0);
    ui.label(RichText::new(format!("UDP {}:{}", s.udp_host, s.udp_port)).color(DIM).small());
    ui.label(RichText::new("UDP address fixed for the session — restart to change.").color(DIM).small());
    c
}
