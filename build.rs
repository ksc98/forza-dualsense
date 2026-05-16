fn main() {
    #[cfg(windows)]
    {
        // No icon shipped yet — keep build clean. If/when assets/icon.ico
        // appears, this picks it up automatically.
        let icon = std::path::Path::new("assets/icon.ico");
        if icon.exists() {
            let mut res = winres::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            res.set("ProductName", "Forza DualSense");
            res.set("FileDescription", "DualSense adaptive triggers for Forza Horizon");
            let _ = res.compile();
        }
    }
}
