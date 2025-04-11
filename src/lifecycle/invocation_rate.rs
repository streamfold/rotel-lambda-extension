use std::time::SystemTime;

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
            return
        }

        let delta_millis = now_millis - self.last_time_millis;
        if delta_millis >= RESET_LENGTH_MILLIS || self.count == 0 {
            self.value = delta_millis as f64;
            self.last_time_millis = now_millis;
            self.count = 1;
            return
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
            return None
        }

        Some((self.value as u64) < rate_millis)
    }
}