//! Bounded guest PCM handoff; all SDL audio work stays on the main thread.
use crate::settings::AudioSettings;
use iris::host_audio::{OutputFactory, PcmStream};
use sdl3::audio::{AudioFormat, AudioSpec, AudioStream, AudioStreamOwner};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Inbox(Mutex<Option<PcmStream>>);

impl std::fmt::Debug for Inbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Prismatic audio inbox")
    }
}

impl OutputFactory for Inbox {
    fn open(&self, stream: PcmStream) -> Result<(), String> {
        let previous = self.0.lock().unwrap().replace(stream);
        drop(previous);
        Ok(())
    }
}

trait Playback {
    fn queued_bytes(&self) -> Result<usize, String>;
    fn clear(&self) -> Result<(), String>;
    fn set_gain(&self, gain: f32) -> Result<(), String>;
    fn put(&self, samples: &[i16]) -> Result<(), String>;
}

impl Playback for AudioStreamOwner {
    fn queued_bytes(&self) -> Result<usize, String> {
        AudioStream::queued_bytes(self)
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
    }
    fn clear(&self) -> Result<(), String> {
        AudioStream::clear(self).map_err(|e| e.to_string())
    }
    fn set_gain(&self, gain: f32) -> Result<(), String> {
        AudioStream::set_gain(self, gain).map_err(|e| e.to_string())
    }
    fn put(&self, samples: &[i16]) -> Result<(), String> {
        self.put_data_i16(samples).map_err(|e| e.to_string())
    }
}

// 40 ms per UI tick, no more than 60 ms queued in SDL. Iris's ring holds at
// most 100 ms. Stalls shed old samples rather than accumulate delayed sound.
const BATCH_SAMPLES: usize = 48_000 * 2 * 40 / 1000;
const MAX_QUEUED_BYTES: usize = 48_000 * 2 * 2 * 60 / 1000;

#[derive(Default)]
pub struct GuestAudio {
    inbox: Option<Arc<Inbox>>,
    input: Option<PcmStream>,
    output: Option<Box<dyn Playback>>,
    settings: AudioSettings,
    audible: bool,
    unavailable: bool,
    reported_signal: bool,
}

impl GuestAudio {
    pub fn pump(
        &mut self,
        sdl: &sdl3::Sdl,
        inbox: Option<&Arc<Inbox>>,
        active: bool,
        settings: AudioSettings,
    ) -> Result<(), String> {
        self.pump_with(inbox, active, settings, |rate| {
            let spec = AudioSpec {
                freq: Some(rate as i32),
                channels: Some(2),
                format: Some(AudioFormat::s16_sys()),
            };
            let stream = sdl
                .audio()
                .and_then(|audio| {
                    audio
                        .default_playback_device()
                        .open_device_stream(Some(&spec))
                })
                .map_err(|e| e.to_string())?;
            stream.resume().map_err(|e| e.to_string())?;
            iris::diagnostics::stderr(format_args!(
                "Prismatic: guest audio through SDL3 at {rate} Hz"
            ));
            Ok(Box::new(stream))
        })
    }

    fn pump_with(
        &mut self,
        inbox: Option<&Arc<Inbox>>,
        active: bool,
        settings: AudioSettings,
        open: impl FnOnce(u32) -> Result<Box<dyn Playback>, String>,
    ) -> Result<(), String> {
        let same = match (&self.inbox, inbox) {
            (Some(current), Some(next)) => Arc::ptr_eq(current, next),
            (None, None) => true,
            _ => false,
        };
        if !same {
            *self = Self {
                inbox: inbox.cloned(),
                ..Self::default()
            };
        }
        let Some(inbox) = &self.inbox else {
            return Ok(());
        };
        let fresh = inbox.0.lock().unwrap().take();
        let fresh_input = fresh.is_some();
        if let Some(input) = fresh {
            self.input = Some(input);
            self.output = None;
            self.reported_signal = false;
        }
        let audible = active && !settings.muted && settings.volume > 0;
        if self.settings.muted && !settings.muted {
            self.unavailable = false; // Retry a missing device when unmuted.
        }
        let changed = audible != self.audible || settings != self.settings;
        self.settings = settings;
        self.audible = audible;
        let result = self.play(audible, changed && !fresh_input, open);
        if result.is_err() {
            self.output = None;
            self.unavailable = true;
            if let Some(input) = &mut self.input {
                input.discard(usize::MAX);
            }
        }
        result
    }

    fn play(
        &mut self,
        audible: bool,
        changed: bool,
        open: impl FnOnce(u32) -> Result<Box<dyn Playback>, String>,
    ) -> Result<(), String> {
        let Some(input) = &mut self.input else {
            return Ok(());
        };
        if input.is_closed() {
            self.output = None;
            self.input = None;
            return Ok(());
        }
        if !audible || changed || self.unavailable {
            input.discard(usize::MAX);
            if let Some(output) = &self.output {
                output.clear()?;
            }
            return Ok(());
        }
        let pending = input.pending_samples();
        if pending == 0 {
            return Ok(());
        }
        input.discard(pending.saturating_sub(BATCH_SAMPLES));
        let mut samples = [0; BATCH_SAMPLES];
        let count = input.read(&mut samples);
        if self.output.is_none() {
            self.output = Some(open(input.sample_rate())?);
        }
        let output = self.output.as_ref().unwrap();
        if output.queued_bytes()?.saturating_add(count * 2) > MAX_QUEUED_BYTES {
            output.clear()?;
        }
        output.set_gain(f32::from(self.settings.volume) / 100.0)?;
        output.put(&samples[..count])?;
        if !self.reported_signal && samples[..count].iter().any(|&sample| sample != 0) {
            self.reported_signal = true;
            iris::diagnostics::stderr(format_args!(
                "Prismatic: non-silent PCM from {} submitted to SDL3",
                input.source()
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::atomic::{AtomicBool, AtomicU64},
    };

    #[derive(Default)]
    struct Recorded {
        samples: Vec<i16>,
        gain: f32,
    }
    struct Fake(Rc<RefCell<Recorded>>);
    impl Drop for Fake {
        fn drop(&mut self) {
            self.0.borrow_mut().samples.clear();
        }
    }
    impl Playback for Fake {
        fn queued_bytes(&self) -> Result<usize, String> {
            Ok(self.0.borrow().samples.len() * 2)
        }
        fn clear(&self) -> Result<(), String> {
            self.0.borrow_mut().samples.clear();
            Ok(())
        }
        fn set_gain(&self, gain: f32) -> Result<(), String> {
            self.0.borrow_mut().gain = gain;
            Ok(())
        }
        fn put(&self, samples: &[i16]) -> Result<(), String> {
            self.0.borrow_mut().samples.extend_from_slice(samples);
            Ok(())
        }
    }

    fn source(inbox: &Arc<Inbox>) -> iris::host_audio::HostOut {
        let cfg = iris::config::AudioConfig {
            output: Some(inbox.clone()),
            ..Default::default()
        };
        iris::host_audio::open_output(
            &cfg,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(true)),
            |_| 9600,
            "test",
            "IRIS_TEST_NO_CAPTURE",
        )
        .unwrap()
    }
    fn send(out: &mut iris::host_audio::HostOut, pair: [i16; 2]) {
        for sample in pair {
            out.producer.push(sample).unwrap();
        }
    }
    fn tick(
        player: &mut GuestAudio,
        inbox: &Arc<Inbox>,
        active: bool,
        settings: AudioSettings,
        recorded: &Rc<RefCell<Recorded>>,
    ) {
        player
            .pump_with(Some(inbox), active, settings, |_| {
                Ok(Box::new(Fake(recorded.clone())))
            })
            .unwrap();
    }

    #[test]
    fn pause_mute_volume_and_machine_switch_discard_old_audio() {
        let inbox = Arc::new(Inbox::default());
        let mut out = source(&inbox);
        let mut player = GuestAudio::default();
        let recorded = Rc::new(RefCell::new(Recorded::default()));
        let mut settings = AudioSettings::default();
        tick(&mut player, &inbox, true, settings, &recorded);
        assert!(player.output.is_none()); // No device until there are samples.
        send(&mut out, [123, -456]);
        tick(&mut player, &inbox, true, settings, &recorded);
        assert_eq!(recorded.borrow().samples, [123, -456]);
        send(&mut out, [1, 2]);
        tick(&mut player, &inbox, false, settings, &recorded);
        assert!(recorded.borrow().samples.is_empty());
        assert_eq!(player.input.as_ref().unwrap().pending_samples(), 0);
        send(&mut out, [3, 4]);
        tick(&mut player, &inbox, true, settings, &recorded);
        assert!(recorded.borrow().samples.is_empty());
        send(&mut out, [5, 6]);
        tick(&mut player, &inbox, true, settings, &recorded);
        assert_eq!(recorded.borrow().samples, [5, 6]);
        settings.muted = true;
        tick(&mut player, &inbox, true, settings, &recorded);
        send(&mut out, [7, 8]);
        tick(&mut player, &inbox, true, settings, &recorded);
        assert!(recorded.borrow().samples.is_empty());
        settings.muted = false;
        settings.volume = 50;
        tick(&mut player, &inbox, true, settings, &recorded);
        send(&mut out, [9, 10]);
        tick(&mut player, &inbox, true, settings, &recorded);
        assert_eq!(recorded.borrow().samples, [9, 10]);
        assert_eq!(recorded.borrow().gain, 0.5);

        // A new machine gets a new inbox. Its stream cannot replay the old one.
        let next = Arc::new(Inbox::default());
        let mut next_out = source(&next);
        let next_recorded = Rc::new(RefCell::new(Recorded::default()));
        tick(&mut player, &next, true, settings, &next_recorded);
        assert!(recorded.borrow().samples.is_empty());
        send(&mut next_out, [11, 12]);
        tick(&mut player, &next, true, settings, &next_recorded);
        assert_eq!(next_recorded.borrow().samples, [11, 12]);
        drop(next_out);
        tick(&mut player, &next, true, settings, &next_recorded);
        assert!(next_recorded.borrow().samples.is_empty());
        assert!(player.input.is_none());
    }

    #[test]
    fn stalled_playback_keeps_recent_frames_and_bounds_both_queues() {
        let inbox = Arc::new(Inbox::default());
        let mut out = source(&inbox);
        let mut player = GuestAudio::default();
        let recorded = Rc::new(RefCell::new(Recorded::default()));
        tick(
            &mut player,
            &inbox,
            true,
            AudioSettings::default(),
            &recorded,
        );
        for _ in 0..5 {
            for i in 0..4800 {
                send(&mut out, [i, -i]);
            }
            assert!(out.producer.push(1).is_err());
            tick(
                &mut player,
                &inbox,
                true,
                AudioSettings::default(),
                &recorded,
            );
            let data = recorded.borrow();
            assert!(data.samples.len() * 2 <= MAX_QUEUED_BYTES);
            assert_eq!(&data.samples[data.samples.len() - 2..], &[4799, -4799]);
            assert!(
                data.samples
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .all(|p| p[0] == -p[1])
            );
        }
    }

    #[test]
    fn failed_device_is_reported_once_and_unmute_retries() {
        let inbox = Arc::new(Inbox::default());
        let mut out = source(&inbox);
        let mut player = GuestAudio::default();
        let settings = AudioSettings::default();
        player
            .pump_with(Some(&inbox), true, settings, |_| panic!("No samples yet"))
            .unwrap();
        send(&mut out, [1, 2]);
        assert_eq!(
            player.pump_with(Some(&inbox), true, settings, |_| Err(
                "device missing".into()
            )),
            Err("device missing".into())
        );
        send(&mut out, [3, 4]);
        player
            .pump_with(Some(&inbox), true, settings, |_| {
                panic!("Must not repeatedly reopen")
            })
            .unwrap();
        assert_eq!(player.input.as_ref().unwrap().pending_samples(), 0);
        let recorded = Rc::new(RefCell::new(Recorded::default()));
        tick(
            &mut player,
            &inbox,
            true,
            AudioSettings {
                muted: true,
                ..settings
            },
            &recorded,
        );
        tick(&mut player, &inbox, true, settings, &recorded);
        send(&mut out, [5, 6]);
        tick(&mut player, &inbox, true, settings, &recorded);
        assert_eq!(recorded.borrow().samples, [5, 6]);
    }
}
