//! Device events cross threads as bits; SDL gives each pulse a visible duration.
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub const MAX_CPUS: usize = 16;
pub const NET: u128 = 1 << MAX_CPUS;
pub fn cpu(index: usize) -> u128 {
    if index < MAX_CPUS { 1 << index } else { 0 }
}

#[derive(Default)]
pub struct CpuCounters([u64; MAX_CPUS]);
impl CpuCounters {
    pub fn update(&mut self, cycles: impl IntoIterator<Item = Option<u64>>) -> u128 {
        let mut events = 0;
        for (index, cycles) in cycles.into_iter().take(MAX_CPUS).enumerate() {
            if let Some(cycles) = cycles {
                if self.0[index] != cycles {
                    events |= cpu(index);
                }
                self.0[index] = cycles;
            } else {
                self.0[index] = 0;
            }
        }
        events
    }
}
pub fn disk_read(id: u8) -> u128 {
    1 << (MAX_CPUS as u8 + 1 + (id / 16 * 15 + id % 16 - 1) * 2)
}
pub fn disk_write(id: u8) -> u128 {
    disk_read(id) << 1
}

#[derive(Clone, Default)]
pub struct Activity(Arc<[AtomicU64; 2]>);
impl Activity {
    pub fn signal(&self, bits: u128) {
        self.0[0].fetch_or(bits as u64, Ordering::Relaxed);
        self.0[1].fetch_or((bits >> 64) as u64, Ordering::Relaxed);
    }
    pub fn take(&self) -> u128 {
        u128::from(self.0[0].swap(0, Ordering::Relaxed))
            | (u128::from(self.0[1].swap(0, Ordering::Relaxed)) << 64)
    }
}

pub struct Leds {
    until: [Option<Instant>; 128],
    pub lit: u128,
}
impl Default for Leds {
    fn default() -> Self {
        Self {
            until: [None; 128],
            lit: 0,
        }
    }
}
impl Leds {
    pub fn update(&mut self, events: u128, now: Instant) -> bool {
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
    fn cpu_activity_tracks_each_counter_independently() {
        let mut counters = CpuCounters::default();
        assert_eq!(counters.update([Some(0), None]), 0);
        assert_eq!(counters.update([Some(10), None]), cpu(0));
        assert_eq!(counters.update([Some(10), Some(20)]), cpu(1));
        assert_eq!(counters.update([Some(15), Some(20)]), cpu(0));
        assert_eq!(counters.update([Some(15), Some(20)]), 0);
        assert_eq!(counters.update([Some(16), Some(21)]), cpu(0) | cpu(1));
        let now = Instant::now();
        let mut leds = Leds::default();
        leds.update(cpu(0) | cpu(1), now);
        leds.update(cpu(1), now + Duration::from_millis(151));
        assert_eq!(leds.lit, cpu(1));
    }

    #[test]
    fn all_thirty_targets_have_distinct_read_and_write_lamps() {
        let mut bits = (0..MAX_CPUS).fold(NET, |bits, i| bits | cpu(i));
        for controller in 0..2 {
            for id in 1..=15 {
                let slot = controller * 16 + id;
                for signal in [disk_read(slot), disk_write(slot)] {
                    assert_eq!(bits & signal, 0);
                    bits |= signal;
                }
            }
        }
        let mut leds = Leds::default();
        leds.update(bits, Instant::now());
        assert_eq!(leds.lit, bits);
        assert_eq!(bits.count_ones(), (MAX_CPUS + 61) as u32);
        let activity = Activity::default();
        activity.signal(bits);
        assert_eq!(activity.take(), bits);
    }

    #[test]
    fn pulses_are_combined_consumed_and_expire_without_new_frames() {
        let source = Activity::default();
        source.signal(cpu(0) | disk_read(1));
        source.clone().signal(disk_write(7));
        let mut leds = Leds::default();
        let now = Instant::now();
        assert!(leds.update(source.take(), now));
        assert_eq!(leds.lit, cpu(0) | disk_read(1) | disk_write(7));
        assert_eq!(source.take(), 0);
        assert!(!leds.update(0, now + Duration::from_millis(100)));
        assert!(leds.update(0, now + Duration::from_millis(151)));
        assert_eq!(leds.lit, 0);
    }
}
