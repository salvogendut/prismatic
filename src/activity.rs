//! Device events cross threads as bits; SDL gives each pulse a visible duration.
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub const CPU: u64 = 1;
pub const NET: u64 = 2;
pub fn disk_read(id: u8) -> u64 {
    1 << (id + 1)
}
pub fn disk_write(id: u8) -> u64 {
    1 << (id + 8)
}

#[derive(Clone, Default)]
pub struct Activity(Arc<AtomicU64>);
impl Activity {
    pub fn signal(&self, bits: u64) {
        self.0.fetch_or(bits, Ordering::Relaxed);
    }
    pub fn take(&self) -> u64 {
        self.0.swap(0, Ordering::Relaxed)
    }
}

#[derive(Default)]
pub struct Leds {
    until: [Option<Instant>; 16],
    pub lit: u64,
}
impl Leds {
    pub fn update(&mut self, events: u64, now: Instant) -> bool {
        let previous = self.lit;
        self.lit = 0;
        for (bit, until) in self.until.iter_mut().enumerate() {
            if events & (1 << bit) != 0 {
                *until = Some(now + Duration::from_millis(150));
            }
            if until.is_some_and(|deadline| now < deadline) {
                self.lit |= 1 << bit;
            }
        }
        self.lit != previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pulses_are_combined_consumed_and_expire_without_new_frames() {
        let source = Activity::default();
        source.signal(CPU | disk_read(1));
        source.clone().signal(disk_write(7));
        let mut leds = Leds::default();
        let now = Instant::now();
        assert!(leds.update(source.take(), now));
        assert_eq!(leds.lit, CPU | disk_read(1) | disk_write(7));
        assert_eq!(source.take(), 0);
        assert!(!leds.update(0, now + Duration::from_millis(100)));
        assert!(leds.update(0, now + Duration::from_millis(151)));
        assert_eq!(leds.lit, 0);
    }
}
