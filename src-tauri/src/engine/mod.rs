use std::ffi::OsString;
use std::path::PathBuf;

pub mod singbox;
pub mod xray;

pub use crate::subscriptions::EngineKind;

#[derive(Clone)]
pub struct LaunchSpec {
    pub engine: EngineKind,
    pub binary: PathBuf,
    pub args: Vec<OsString>,
    pub config_path: PathBuf,
    pub controller_url: Option<String>,
    pub profile_key: Option<String>,
    pub profile_name: Option<String>,
}
