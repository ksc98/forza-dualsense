//! System-tray integration: persistent tray icon with a "Show / Quit"
//! menu, plus minimize-to-tray hide behaviour wired up from the egui
//! update loop. The tray crate runs its own message hook on Windows;
//! we just create the icon on the UI thread and poll its event
//! channels each frame.

#[cfg(windows)]
mod imp {
    use anyhow::Result;
    use tray_icon::{
        menu::{Menu, MenuId, MenuItem},
        TrayIcon, TrayIconBuilder,
    };

    /// Holds the tray icon plus the menu-item IDs so the GUI knows which
    /// command was clicked. The `TrayIcon` is kept alive for the life of
    /// the app — dropping it removes the icon from the tray.
    pub struct Tray {
        _icon: TrayIcon,
        pub show_id: MenuId,
        pub quit_id: MenuId,
    }

    impl Tray {
        pub fn build() -> Result<Self> {
            let png = include_bytes!("../assets/icon.png");
            let img = image::load_from_memory(png)?.to_rgba8();
            let (w, h) = img.dimensions();
            let icon = tray_icon::Icon::from_rgba(img.into_raw(), w, h)?;

            let menu = Menu::new();
            let show = MenuItem::new("Show window", true, None);
            let quit = MenuItem::new("Quit", true, None);
            menu.append(&show)?;
            menu.append(&quit)?;
            let show_id = show.id().clone();
            let quit_id = quit.id().clone();

            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_icon(icon)
                .with_tooltip("Forza DualSense")
                .build()?;

            Ok(Self {
                _icon: tray,
                show_id,
                quit_id,
            })
        }
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
mod imp {
    /// Non-Windows stub: tray support is a Windows-only convenience.
    /// Building the stub fails so the GUI cleanly logs "tray unavailable"
    /// and runs without one.
    pub struct Tray;

    impl Tray {
        pub fn build() -> anyhow::Result<Self> {
            anyhow::bail!("system tray is only implemented on Windows")
        }
    }
}

#[allow(unused_imports)]
pub use imp::Tray;
