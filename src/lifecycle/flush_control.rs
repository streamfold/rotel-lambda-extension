use crate::lifecycle::flush_control::FlushMode::{AfterCall, Periodic};
use crate::lifecycle::invocation_rate::InvocationRate;
use std::sync::{Arc, Mutex};

// Default flush interval that captures any long duration
// lambda invocations. If we flush at the end or periodically at the
// beginning of an invocation, then this interval is reset
pub const DEFAULT_FLUSH_INTERVAL_MILLIS: u64 = 60 * 1_000;

// Interval used when flushing periodically at the beginning of an
// invocation.
const PERIODIC_FLUSH_RATE_MILLIS: u64 = 20 * 1_000;

// If the invocation rate is faster than this, switch to periodically
// flushing on an interval timer. Otherwise we'll flush at the end of
// an invocation.
const ACTIVE_INVOCATION_RATE_MILLIS: u64 = 60 * 1_000;

pub trait Clock {
    fn now(&self) -> u64;
}

pub struct FlushControl<C: Clock> {
    rate: InvocationRate,
    inner: Arc<Mutex<Inner>>,
    clock: C,
}

struct Inner {
    last_flush: u64,
}

pub enum FlushMode<C: Clock> {
    AfterCall,
    Periodic(PeriodicFlushControl<C>),
}

pub struct PeriodicFlushControl<C: Clock> {
    inner: Arc<Mutex<Inner>>,
    clock: C,
}

impl<C: Clock> PeriodicFlushControl<C> {
    pub fn should_flush(&mut self) -> bool {
        let now_millis = self.clock.now();
        let mut g = self.inner.lock().unwrap();

        if now_millis > g.last_flush && (now_millis - g.last_flush) > PERIODIC_FLUSH_RATE_MILLIS {
            g.last_flush = now_millis;
            true
        } else {
            false
        }
    }
}

impl<C: Clock + Clone> FlushControl<C> {
    pub fn new(clock: C) -> Self {
        Self {
            clock: clock.clone(),
            rate: InvocationRate::default(),
            inner: Arc::new(Mutex::new(Inner {
                last_flush: clock.now(),
            })),
        }
    }

    pub fn pick(&mut self) -> FlushMode<C> {
        let now_millis = self.clock.now();
        self.rate.add(now_millis);

        let mode = match self.rate.is_faster_than(ACTIVE_INVOCATION_RATE_MILLIS) {
            // Not initialized, stick to flush per call
            None => AfterCall,

            Some(is_faster) => match is_faster {
                true => Periodic(PeriodicFlushControl {
                    clock: self.clock.clone(),
                    inner: self.inner.clone(),
                }),
                false => AfterCall,
            },
        };

        match mode {
            AfterCall => {
                // Update last flush time so that if we switch to periodic, we don't
                // immediately attempt a flush because last_flush hasn't been updated
                let mut g = self.inner.lock().unwrap();
                g.last_flush = now_millis;
            },
            _ => {},
        }

        mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    // Test implementation of the Clock trait
    #[derive(Clone)]
    struct TestClock {
        time: Rc<Cell<u64>>,
    }

    impl TestClock {
        fn new(initial_time: u64) -> Self {
            Self { time: Rc::new(Cell::new(initial_time)) }
        }

        fn advance(&self, millis: u64) {
            self.time.set(self.time.get() + millis);
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> u64 {
            self.time.get()
        }
    }

    #[test]
    fn test_initial_state() {
        let clock = TestClock::new(1000);
        let mut flush_control = FlushControl::new(clock);

        // Initially, we should get AfterCall mode since InvocationRate isn't warmed up
        match flush_control.pick() {
            FlushMode::AfterCall => {},
            _ => panic!("Expected AfterCall mode initially"),
        }
    }

    #[test]
    fn test_after_call_mode_for_slow_invocations() {
        let clock = TestClock::new(1000);
        let mut flush_control = FlushControl::new(clock.clone());

        // Complete warmup with slow invocations (greater than ACTIVE_INVOCATION_RATE_MILLIS)
        for i in 1..=20 {
            clock.advance(ACTIVE_INVOCATION_RATE_MILLIS + 1000); // Very slow rate
            let mode = flush_control.pick();

            // During warmup, we should still get AfterCall
            if i < 20 {
                match mode {
                    FlushMode::AfterCall => {},
                    _ => panic!("Expected AfterCall mode during warmup"),
                }
            } else {
                // After warmup with slow invocations, we should still get AfterCall
                match mode {
                    FlushMode::AfterCall => {},
                    _ => panic!("Expected AfterCall mode for slow invocations"),
                }
            }
        }
    }

    #[test]
    fn test_periodic_mode_for_fast_invocations() {
        let clock = TestClock::new(1000);
        let mut flush_control = FlushControl::new(clock.clone());

        // Complete warmup with fast invocations (less than ACTIVE_INVOCATION_RATE_MILLIS)
        for _i in 1..=20 {
            clock.advance(ACTIVE_INVOCATION_RATE_MILLIS / 2); // Fast rate
            let _ = flush_control.pick();
        }

        // One more pick() after warmup should give us Periodic mode
        match flush_control.pick() {
            FlushMode::Periodic(_) => {},
            _ => panic!("Expected Periodic mode for fast invocations"),
        }
    }

    #[test]
    fn test_transition_from_periodic_to_after_call() {
        let clock = TestClock::new(1000);
        let mut flush_control = FlushControl::new(clock.clone());

        // Warm up with fast invocations
        for _ in 1..=20 {
            clock.advance(ACTIVE_INVOCATION_RATE_MILLIS / 2);
            let _ = flush_control.pick();
        }

        // Should be in Periodic mode now
        match flush_control.pick() {
            FlushMode::Periodic(_) => {},
            _ => panic!("Expected to be in Periodic mode"),
        }

        // Now switch to slow invocations
        for _ in 1..=10 {
            clock.advance(ACTIVE_INVOCATION_RATE_MILLIS * 2);
            let mode = flush_control.pick();

            // Eventually should switch back to AfterCall
            if let FlushMode::AfterCall = mode {
                return; // Test passed
            }
        }

        panic!("Failed to transition back to AfterCall mode");
    }

    #[test]
    fn test_periodic_flush_control() {
        let clock = TestClock::new(1000);
        let mut flush_control = FlushControl::new(clock.clone());

        // Warm up with fast invocations to get to Periodic mode
        for _ in 1..=20 {
            clock.advance(PERIODIC_FLUSH_RATE_MILLIS / 2);
            let _ = flush_control.pick();
        }

        // Get the PeriodicFlushControl
        let mut periodic_control = match flush_control.pick() {
            FlushMode::Periodic(control) => control,
            _ => panic!("Expected to get PeriodicFlushControl"),
        };

        // Initially, should not flush (time elapsed is 0)
        assert!(!periodic_control.should_flush());

        // Advance time but still below threshold
        clock.advance(100);
        assert!(!periodic_control.should_flush());

        // Advance time past threshold
        clock.advance(PERIODIC_FLUSH_RATE_MILLIS);
        assert!(periodic_control.should_flush());

        // After flushing, should not flush again immediately
        assert!(!periodic_control.should_flush());

        // After another interval, should flush again
        clock.advance(PERIODIC_FLUSH_RATE_MILLIS + 1);
        assert!(periodic_control.should_flush());
    }

    #[test]
    fn test_multiple_periodic_flush_controls_share_state() {
        let clock = TestClock::new(1000);
        let mut flush_control = FlushControl::new(clock.clone());

        // Warm up with fast invocations
        for _ in 1..=20 {
            clock.advance(ACTIVE_INVOCATION_RATE_MILLIS / 2);
            let _ = flush_control.pick();
        }

        // Get first periodic control
        let mut periodic_control1 = match flush_control.pick() {
            FlushMode::Periodic(control) => control,
            _ => panic!("Expected to get PeriodicFlushControl"),
        };

        // Get second periodic control
        let mut periodic_control2 = match flush_control.pick() {
            FlushMode::Periodic(control) => control,
            _ => panic!("Expected to get PeriodicFlushControl"),
        };

        // Advance time past threshold
        clock.advance(PERIODIC_FLUSH_RATE_MILLIS + 1);

        // First control should indicate a flush is needed
        assert!(periodic_control1.should_flush());

        // Second control should not indicate a flush is needed
        // since the last_flush was updated by the first control
        assert!(!periodic_control2.should_flush());

        // After waiting another interval, both should be able to flush
        clock.advance(PERIODIC_FLUSH_RATE_MILLIS + 1);
        assert!(periodic_control2.should_flush());
        assert!(!periodic_control1.should_flush()); // First one affected by second one's flush
    }
}