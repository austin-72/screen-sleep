//! The decision logic, kept free of any OS calls so it can be unit tested.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Nothing to do; the screen stays on.
    Active,
    /// The countdown is showing.
    Warning { seconds_left: u32 },
    /// The idle timeout has passed; the screen should be off.
    Off,
}

/// The timing rules in force right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub enabled: bool,
    pub timeout_ms: u64,
    /// Length of the countdown shown before the timeout; 0 for none.
    pub warning_ms: u64,
}

/// Decides what phase we should be in, given how long it has been since any
/// activity.
///
/// This is a pure function of its inputs (no "previous state"), using
/// thresholds rather than exact matches, so a late or skipped timer tick can
/// never cause the timeout to be missed.
pub fn phase(timing: &Timing, idle_ms: u64) -> Phase {
    if !timing.enabled {
        return Phase::Active;
    }
    if idle_ms >= timing.timeout_ms {
        Phase::Off
    } else if idle_ms + timing.warning_ms >= timing.timeout_ms {
        let seconds_left = (timing.timeout_ms - idle_ms).div_ceil(1_000) as u32;
        Phase::Warning { seconds_left }
    } else {
        Phase::Active
    }
}

/// How long it has been since anything happened: the user touched the
/// keyboard or mouse, or (if known) something else counted as activity,
/// such as media playing.
pub fn effective_idle_ms(input_idle_ms: u64, since_other_activity_ms: Option<u64>) -> u64 {
    since_other_activity_ms.map_or(input_idle_ms, |other| input_idle_ms.min(other))
}

/// Where a pulse is in its cycle: 0.0 at the start, rising smoothly to 1.0
/// halfway through and falling back to 0.0. A zero period never pulses.
pub fn pulse_level(elapsed_ms: u64, period_ms: u64) -> f32 {
    if period_ms == 0 {
        return 0.0;
    }
    let turn = (elapsed_ms % period_ms) as f32 / period_ms as f32;
    0.5 - 0.5 * (turn * std::f32::consts::TAU).cos()
}

/// While checking whether a sound is sustained, a silence longer than this
/// ends the attempt. Short enough to separate one chime from the next, long
/// enough to ride over the pauses within speech and music.
const VERIFY_GAP_MS: u64 = 600;
/// Once media is playing, silences up to this long (the gap between tracks,
/// a quiet scene) don't count as it having stopped.
const PLAYING_GAP_MS: u64 = 3_000;

/// Tells sustained sound (music, a video) apart from brief sounds such as
/// notification chimes, from periodic "is anything audible?" samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SoundTracker {
    #[default]
    Silent,
    /// Sound has started, but hasn't yet lasted long enough to count.
    Verifying {
        since_ms: u64,
        last_heard_ms: u64,
    },
    Playing {
        last_heard_ms: u64,
    },
}

impl SoundTracker {
    /// Feeds in one sample taken at `now_ms` (any monotonic clock). Sound
    /// counts as media once it has been continuous for `min_ms`.
    pub fn update(self, now_ms: u64, audible: bool, min_ms: u64) -> SoundTracker {
        use SoundTracker::*;
        let fresh = if min_ms == 0 {
            Playing { last_heard_ms: now_ms }
        } else {
            Verifying {
                since_ms: now_ms,
                last_heard_ms: now_ms,
            }
        };
        match (self, audible) {
            (Silent, false) => Silent,
            (Silent, true) => fresh,
            (
                Verifying {
                    since_ms,
                    last_heard_ms,
                },
                true,
            ) => {
                if now_ms.saturating_sub(last_heard_ms) > VERIFY_GAP_MS {
                    // Too long since the last sample to call it continuous
                    // (e.g. the PC was asleep in between): start over.
                    fresh
                } else if now_ms.saturating_sub(since_ms) >= min_ms {
                    Playing { last_heard_ms: now_ms }
                } else {
                    Verifying {
                        since_ms,
                        last_heard_ms: now_ms,
                    }
                }
            }
            (Verifying { last_heard_ms, .. }, false) => {
                if now_ms.saturating_sub(last_heard_ms) > VERIFY_GAP_MS {
                    Silent
                } else {
                    self
                }
            }
            (Playing { .. }, true) => Playing { last_heard_ms: now_ms },
            (Playing { last_heard_ms }, false) => {
                if now_ms.saturating_sub(last_heard_ms) > PLAYING_GAP_MS {
                    Silent
                } else {
                    self
                }
            }
        }
    }

    pub fn is_playing(self) -> bool {
        matches!(self, SoundTracker::Playing { .. })
    }

    /// While verifying, samples need to come quickly enough to notice the
    /// sound stopping.
    pub fn needs_fast_sampling(self) -> bool {
        matches!(self, SoundTracker::Verifying { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;

    fn timing() -> Timing {
        Timing {
            enabled: true,
            timeout_ms: 15 * MIN,
            warning_ms: 30_000,
        }
    }

    #[test]
    fn active_before_warning_window() {
        assert_eq!(phase(&timing(), 0), Phase::Active);
        assert_eq!(phase(&timing(), 15 * MIN - 30_001), Phase::Active);
    }

    #[test]
    fn warning_counts_down() {
        let w = |idle| phase(&timing(), idle);
        assert_eq!(w(15 * MIN - 30_000), Phase::Warning { seconds_left: 30 });
        assert_eq!(w(15 * MIN - 29_999), Phase::Warning { seconds_left: 30 });
        assert_eq!(w(15 * MIN - 29_000), Phase::Warning { seconds_left: 29 });
        assert_eq!(w(15 * MIN - 1), Phase::Warning { seconds_left: 1 });
    }

    #[test]
    fn off_at_and_after_timeout() {
        assert_eq!(phase(&timing(), 15 * MIN), Phase::Off);
        // A tick that arrives late must still turn the screen off.
        assert_eq!(phase(&timing(), 15 * MIN + 4_321), Phase::Off);
        assert_eq!(phase(&timing(), 40 * MIN), Phase::Off);
    }

    #[test]
    fn disabled_is_always_active() {
        let disabled = Timing {
            enabled: false,
            ..timing()
        };
        assert_eq!(phase(&disabled, 99 * MIN), Phase::Active);
    }

    #[test]
    fn other_activity_counts_like_input() {
        assert_eq!(effective_idle_ms(20 * MIN, None), 20 * MIN);
        // Media still playing: no idle time at all, however long since the last key press.
        assert_eq!(phase(&timing(), effective_idle_ms(120 * MIN, Some(0))), Phase::Active);
        // After it stops, the full timeout (and countdown) runs from that moment.
        assert_eq!(
            phase(&timing(), effective_idle_ms(120 * MIN, Some(14 * MIN))),
            Phase::Active
        );
        assert_eq!(
            phase(&timing(), effective_idle_ms(120 * MIN, Some(15 * MIN))),
            Phase::Off
        );
        // Recent input wins over old media.
        assert_eq!(effective_idle_ms(5, Some(60 * MIN)), 5);
    }

    #[test]
    fn no_warning_when_warning_is_zero() {
        let t = Timing {
            warning_ms: 0,
            ..timing()
        };
        assert_eq!(phase(&t, 15 * MIN - 1), Phase::Active);
        assert_eq!(phase(&t, 15 * MIN), Phase::Off);
    }

    #[test]
    fn warning_longer_than_timeout_starts_immediately() {
        let t = Timing {
            enabled: true,
            timeout_ms: MIN,
            warning_ms: 300_000,
        };
        assert_eq!(phase(&t, 0), Phase::Warning { seconds_left: 60 });
    }

    #[test]
    fn pulse_rises_and_falls_smoothly() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(close(pulse_level(0, 1000), 0.0));
        assert!(close(pulse_level(250, 1000), 0.5));
        assert!(close(pulse_level(500, 1000), 1.0));
        assert!(close(pulse_level(750, 1000), 0.5));
        assert!(close(pulse_level(1000, 1000), 0.0));
        assert!(close(pulse_level(123_500, 1000), 1.0));
        assert_eq!(pulse_level(500, 0), 0.0);
    }

    /// Runs a tracker over `(time_ms, audible)` samples, 2 s minimum.
    fn track(samples: &[(u64, bool)]) -> SoundTracker {
        samples
            .iter()
            .fold(SoundTracker::Silent, |t, &(now, audible)| t.update(now, audible, 2_000))
    }

    #[test]
    fn notification_chime_is_ignored() {
        // Audible for 0.6 s, then silent.
        let t = track(&[(0, true), (200, true), (400, true), (600, true), (800, false)]);
        assert!(!t.is_playing() && t.needs_fast_sampling());
        let t = track(&[
            (0, true),
            (200, true),
            (400, true),
            (600, true),
            (800, false),
            (1_400, false),
        ]);
        assert_eq!(t, SoundTracker::Silent);
    }

    #[test]
    fn separate_chimes_do_not_add_up() {
        // Several messages arriving a second or two apart.
        let mut samples = Vec::new();
        for chime in 0..5u64 {
            let start = chime * 1_600;
            samples.extend([(start, true), (start + 200, true), (start + 400, true)]);
            samples.extend((3..8).map(|i| (start + i * 200, false)));
        }
        assert!(!track(&samples).is_playing());
    }

    #[test]
    fn sustained_sound_counts_after_the_minimum() {
        let samples: Vec<_> = (0..=9).map(|i| (i * 200, true)).collect();
        assert!(!track(&samples).is_playing()); // 1.8 s so far
        let samples: Vec<_> = (0..=10).map(|i| (i * 200, true)).collect();
        assert!(track(&samples).is_playing()); // 2.0 s
    }

    #[test]
    fn short_pauses_while_verifying_are_tolerated() {
        // Speech: a 0.4 s pause between words doesn't restart the count.
        let samples: Vec<_> = (0..=10).map(|i| (i * 200, !(4..=5).contains(&i))).collect();
        assert!(track(&samples).is_playing());
    }

    #[test]
    fn playing_survives_gaps_between_tracks_then_stops() {
        let playing = SoundTracker::Playing { last_heard_ms: 10_000 };
        assert!(playing.update(12_000, false, 2_000).is_playing());
        assert!(playing.update(13_000, false, 2_000).is_playing());
        assert_eq!(playing.update(13_001, false, 2_000), SoundTracker::Silent);
        // Slow one-second samples are enough to stay "playing".
        assert!(playing
            .update(11_000, true, 2_000)
            .update(12_000, true, 2_000)
            .is_playing());
    }

    #[test]
    fn samples_far_apart_are_not_continuous() {
        // Audible before and after a long stall (e.g. the PC slept).
        assert!(!track(&[(0, true), (600_000, true)]).is_playing());
    }

    #[test]
    fn zero_minimum_counts_any_sound() {
        assert!(SoundTracker::Silent.update(0, true, 0).is_playing());
    }
}
