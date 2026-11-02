//! Older Iris writes directly to its console; no screen-routing hook exists.
use crate::settings::DebugOutput;
pub const SCREEN_LINES: usize = 12;
pub struct DebugLog;
impl DebugLog {
    pub fn new(_: DebugOutput) -> Self {
        Self
    }
    pub fn set_output(&self, _: DebugOutput) {}
    pub fn take_update(&self) -> Option<Vec<String>> {
        None
    }
}
pub fn install(_: std::sync::Arc<DebugLog>) {}
pub fn stderr(message: std::fmt::Arguments<'_>) {
    eprintln!("{message}");
}
