use std::sync::Mutex;
use std::time::Instant;

use crate::config::CircuitBreakerPolicy;
use crate::error::{NetError, Result};

#[derive(Debug, Default)]
struct CircuitState {
    consecutive_failures: usize,
    opened_at: Option<Instant>,
    half_open_in_flight: usize,
}

pub struct CircuitBreaker {
    policy: CircuitBreakerPolicy,
    state: Mutex<CircuitState>,
}

impl CircuitBreaker {
    pub fn new(policy: CircuitBreakerPolicy) -> Self {
        Self {
            policy,
            state: Mutex::new(CircuitState::default()),
        }
    }

    pub fn allow(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| NetError::Transport("circuit mutex poisoned".into()))?;
        if let Some(opened_at) = state.opened_at {
            if opened_at.elapsed() < self.policy.open_window {
                return Err(NetError::CircuitOpen(format!(
                    "cooldown active for {:?}",
                    self.policy.open_window - opened_at.elapsed()
                )));
            }
            if state.half_open_in_flight >= self.policy.half_open_permits {
                return Err(NetError::CircuitOpen(
                    "half-open concurrency limit reached".into(),
                ));
            }
            state.half_open_in_flight += 1;
        }
        Ok(())
    }

    pub fn on_success(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| NetError::Transport("circuit mutex poisoned".into()))?;
        state.consecutive_failures = 0;
        state.opened_at = None;
        state.half_open_in_flight = 0;
        Ok(())
    }

    pub fn on_failure(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| NetError::Transport("circuit mutex poisoned".into()))?;
        state.consecutive_failures += 1;
        if state.consecutive_failures >= self.policy.failure_threshold {
            state.opened_at = Some(Instant::now());
            state.half_open_in_flight = 0;
        }
        Ok(())
    }
}
