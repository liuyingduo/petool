mod manager;
pub mod types;

#[cfg(target_os = "windows")]
mod win;

pub use manager::{action_from_arguments, execute_desktop_request, is_high_risk_action};
