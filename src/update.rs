//! Self-update against GitHub Releases.
//!
//! On launch the app checks the `latest` release for this repo. If a
//! newer version is published it downloads the matching archive,
//! replaces the running binary in place, and surfaces a "restart to
//! apply" banner. The check is opt-out via Settings or `--no-update`.

use serde::Serialize;

pub const REPO_OWNER: &str = "ksc98";
pub const REPO_NAME: &str = "forza-dualsense";
pub const BIN_NAME: &str = "forza-dualsense";

#[derive(Clone, Debug, Default, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    #[default]
    Idle,
    Disabled,
    Checking,
    UpToDate,
    Applied { version: String },
    Failed { error: String },
}

/// Blocking. Returns the resulting status; never panics.
pub fn check_and_apply() -> Status {
    let current = env!("CARGO_PKG_VERSION");
    let outcome = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .show_download_progress(false)
        .show_output(false)
        .no_confirm(true)
        .current_version(current)
        .build()
        .and_then(|u| u.update());

    match outcome {
        Ok(self_update::Status::UpToDate(_)) => Status::UpToDate,
        Ok(self_update::Status::Updated(v)) => Status::Applied { version: v },
        Err(e) => Status::Failed { error: e.to_string() },
    }
}
