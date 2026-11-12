//! Arc's workflow/evidence extension; OpenCode owns model execution and presentation.
pub const MAX_TASKS: usize = 5;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub mod evidence;
#[cfg(target_os = "macos")]
pub mod native_check;
pub mod process_check;
pub mod protocol;
pub mod workflow;
