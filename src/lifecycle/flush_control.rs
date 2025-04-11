use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use crate::lifecycle::flush_control::FlushMode::{AfterCall, Periodic};
use crate::lifecycle::invocation_rate::InvocationRate;

// Controls whether we should flush at the top of a function
// execution. todo: make this configurable
const PERIODIC_FLUSH_RATE_MILLIS: u64 = 20_000;

// If the invocation rate is faster than this, switch to periodically
// flushing on an interval timer. Otherwise we'll flush at the end of
// an invocation.
const ACTIVE_INVOCATION_RATE_MILLIS: u64 = 60_000;

pub struct FlushControl {
    rate: InvocationRate,
    inner: Arc<Mutex<Inner>>
}

struct Inner {
    last_flush: u64
}

pub enum FlushMode {
    AfterCall,
    Periodic(PeriodicFlushControl)
}

pub struct PeriodicFlushControl {
    inner: Arc<Mutex<Inner>>
}

impl PeriodicFlushControl {
    pub fn should_flush(&mut self) -> bool {
        let now_millis = now_millis();
        let mut g = self.inner.lock().unwrap();

        if now_millis > g.last_flush && (now_millis - g.last_flush) > PERIODIC_FLUSH_RATE_MILLIS {
            g.last_flush = now_millis;
            true
        } else {
            false
        }
    }
}

impl FlushControl {
    pub fn new() -> Self {
        Self{
            rate: InvocationRate::default(),
            inner: Arc::new(Mutex::new(Inner{last_flush: now_millis()})),
        }
    }

    pub fn pick(&mut self) -> FlushMode {
        let now_millis = now_millis();

        self.rate.add(now_millis);

        match self.rate.is_faster_than(ACTIVE_INVOCATION_RATE_MILLIS) {
            // Not initialized, stick to flush per call
            None => AfterCall,

            Some(is_faster) => match is_faster {
                true => Periodic(PeriodicFlushControl{ inner: self.inner.clone()}),
                false => AfterCall,
            },
        }
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap()
        .as_millis() as u64
}