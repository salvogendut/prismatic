//! UI sounds only. Iris guest audio remains disabled.
use sdl3::{
    audio::{AudioSpec, AudioSpecWAV, AudioStreamOwner},
    iostream::IOStream,
};

const SHUTTER: &[u8] = include_bytes!("../assets/shutter.wav");

struct LoadedShutter {
    stream: AudioStreamOwner,
    sample: AudioSpecWAV,
}

impl LoadedShutter {
    fn new(sdl: &sdl3::Sdl) -> Result<Self, String> {
        let sample = decode_shutter().map_err(|e| e.to_string())?;
        let spec = AudioSpec {
            freq: Some(sample.freq),
            channels: Some(sample.channels as i32),
            format: Some(sample.format),
        };
        let audio = sdl.audio().map_err(|e| e.to_string())?;
        let stream = audio
            .default_playback_device()
            .open_device_stream(Some(&spec))
            .map_err(|e| e.to_string())?;
        stream.resume().map_err(|e| e.to_string())?;
        Ok(Self { stream, sample })
    }
}

#[derive(Default)]
pub struct Shutter {
    loaded: Option<LoadedShutter>,
    unavailable: bool,
}

impl Shutter {
    /// Main-thread-only, called after a screenshot has been published.
    pub fn play(&mut self, sdl: &sdl3::Sdl) -> Result<(), String> {
        if self.unavailable {
            return Ok(());
        }
        let result = (|| {
            if self.loaded.is_none() {
                self.loaded = Some(LoadedShutter::new(sdl)?);
            }
            let loaded = self.loaded.as_ref().unwrap();
            // Repeated shots restart the sample rather than accumulating audio.
            loaded.stream.clear().map_err(|e| e.to_string())?;
            loaded
                .stream
                .put_data(loaded.sample.buffer())
                .map_err(|e| e.to_string())?;
            loaded.stream.flush().map_err(|e| e.to_string())
        })();
        if result.is_err() {
            self.loaded = None;
            self.unavailable = true;
        }
        result
    }
}

fn decode_shutter() -> Result<AudioSpecWAV, sdl3::Error> {
    AudioSpecWAV::load_wav_rw(&mut IOStream::from_bytes(SHUTTER)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_happy_years_shutter_decodes_to_non_silent_pcm() {
        let sample = decode_shutter().unwrap();
        assert_eq!(sample.freq, 44100);
        assert_eq!(sample.channels, 1);
        assert_eq!(sample.buffer().len(), 29952);
        assert!(sample.buffer().iter().any(|&byte| byte != 0));
    }
}
