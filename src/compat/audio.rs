//! This Iris checkout has no frontend PCM output hook.
use std::sync::Arc;
#[derive(Default)]
pub struct Inbox {
    _private: (),
}
#[derive(Default)]
pub struct GuestAudio {
    _private: (),
}
impl GuestAudio {
    pub fn pump(
        &mut self,
        _: &sdl3::Sdl,
        _: Option<&Arc<Inbox>>,
        _: bool,
        _: crate::settings::AudioSettings,
    ) -> Result<(), String> {
        Ok(())
    }
}
