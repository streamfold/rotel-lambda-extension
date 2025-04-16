// If we didn't execute for 5mins, reset
const RESET_LENGTH_MILLIS: u64 = 300 * 1_000;

const DECAY: f64 = 0.07;

const WARMUP_COUNT: u8 = 20;

#[derive(Default, Debug)]
pub struct InvocationRate {
    last_time_millis: u64,
    value: f64,
    count: u8,
}

impl InvocationRate {
    pub fn add(&mut self, now_millis: u64) {
        // invalid, discard
        if now_millis <= self.last_time_millis {
            return;
        }

        let delta_millis = now_millis - self.last_time_millis;

        // If we haven't run in a while, reset our state
        if delta_millis >= RESET_LENGTH_MILLIS {
            self.value = 0.0;
            self.last_time_millis = now_millis;
            self.count = 0;
            return;
        }

        // First time, start value at the first delta
        if self.count == 0 {
            self.value = delta_millis as f64;
            self.last_time_millis = now_millis;
            self.count = 1;
            return;
        }

        let delta_millis = delta_millis as f64;
        self.value = (delta_millis * DECAY) + (self.value * (1.0 - DECAY));
        self.last_time_millis = now_millis;

        if self.count < WARMUP_COUNT {
            self.count += 1;
        }
    }

    pub fn is_faster_than(&self, rate_millis: u64) -> Option<bool> {
        // not ready
        if self.count < WARMUP_COUNT {
            return None;
        }

        Some((self.value as u64) < rate_millis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let rate = InvocationRate::default();
        assert_eq!(rate.last_time_millis, 0);
        assert_eq!(rate.value, 0.0);
        assert_eq!(rate.count, 0);

        // Should return None when not warmed up
        assert_eq!(rate.is_faster_than(100), None);
    }

    #[test]
    fn test_first_invocation() {
        let mut rate = InvocationRate::default();
        rate.add(1000);

        assert_eq!(rate.last_time_millis, 1000);
        assert_eq!(rate.value, 1000.0);
        assert_eq!(rate.count, 1);
        assert_eq!(rate.is_faster_than(100), None); // Still not warmed up
    }

    #[test]
    fn test_warmup_phase() {
        let mut rate = InvocationRate::default();

        // Add 19 invocations (not enough to complete warmup)
        for i in 1..20 {
            rate.add(i * 100);
            assert_eq!(rate.count, i as u8);
            assert_eq!(rate.is_faster_than(50), None); // Still warming up
        }

        // Add the final invocation to complete warmup
        rate.add(2000);
        assert_eq!(rate.count, 20);

        // Now we should get a real result instead of None
        assert!(rate.is_faster_than(50).is_some());
    }

    #[test]
    fn test_reset_on_large_time_gap() {
        let mut rate = InvocationRate::default();

        // Add some initial invocations
        for i in 1..=20 {
            rate.add(i * 100);
        }

        // State before reset
        assert_eq!(rate.count, 20);
        assert!(rate.value > 0.0);

        // Add an invocation with a gap larger than RESET_LENGTH_MILLIS
        rate.add(2000 + RESET_LENGTH_MILLIS + 1);

        // Should have reset
        assert_eq!(rate.value, 0.0);
        assert_eq!(rate.count, 0);
    }

    #[test]
    fn test_steady_state_faster_than_threshold() {
        let mut rate = InvocationRate::default();

        // Complete warmup with small deltas (fast invocations)
        for i in 1..=WARMUP_COUNT {
            rate.add(i as u64 * 50); // 50ms intervals
        }

        // Should be faster than 100ms
        assert_eq!(rate.is_faster_than(100), Some(true));
    }

    #[test]
    fn test_steady_state_slower_than_threshold() {
        let mut rate = InvocationRate::default();

        // Complete warmup with larger deltas (slow invocations)
        for i in 1..=WARMUP_COUNT {
            rate.add(i as u64 * 200); // 200ms intervals
        }

        // Should NOT be faster than 100ms
        assert_eq!(rate.is_faster_than(100), Some(false));
    }

    #[test]
    fn test_discard_invalid_timestamp() {
        let mut rate = InvocationRate::default();

        // Set initial state
        rate.add(1000);
        assert_eq!(rate.last_time_millis, 1000);
        assert_eq!(rate.count, 1);

        // Try to add an earlier timestamp (should be discarded)
        rate.add(500);

        // State should remain unchanged
        assert_eq!(rate.last_time_millis, 1000);
        assert_eq!(rate.count, 1);

        // Same timestamp should also be discarded
        rate.add(1000);
        assert_eq!(rate.last_time_millis, 1000);
        assert_eq!(rate.count, 1);
    }

    #[test]
    fn test_exponential_decay() {
        let mut rate = InvocationRate::default();

        // Add first invocation
        rate.add(1000);
        assert_eq!(rate.value, 1000.0);

        // Add second invocation with 100ms delta
        rate.add(1100);
        let first_value = rate.value;
        assert!(first_value > 0.0);

        // Add third invocation with same delta
        rate.add(1200);
        let second_value = rate.value;

        // Value should be approaching the delta with exponential decay
        assert!(second_value > 0.0);
        assert_ne!(second_value, first_value); // Should have changed

        // After many iterations with the same delta, value should approach
        // a steady state related to that delta
        for i in 3..75 {
            rate.add(1000 + i * 100);
        }

        // Final value should be close to delta * DECAY / (1 - (1 - DECAY))
        // which is just equal to delta * DECAY / DECAY = delta
        let expected_steady_state = 100.0 * DECAY / DECAY;
        let tolerance = 5.0; // Allow some numerical error

        assert!((rate.value - expected_steady_state).abs() < tolerance);
    }

    #[test]
    fn test_changing_rates() {
        let mut rate = InvocationRate::default();

        // Warm up with fast invocations
        for i in 1..=WARMUP_COUNT {
            rate.add(i as u64 * 50);
        }

        // Should be faster than 100ms
        assert_eq!(rate.is_faster_than(100), Some(true));

        // Switch to slow invocations
        for i in 0..10 {
            rate.add((WARMUP_COUNT as u64) * 50 + 1 + i * 200);
        }

        // Should now be slower than 100ms
        assert_eq!(rate.is_faster_than(100), Some(false));
    }
}
