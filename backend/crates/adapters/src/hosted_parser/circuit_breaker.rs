//! Provider/model circuit-breaker state and cooldown responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Default)]
pub(crate) struct CircuitState {
    consecutive_failures: u32,
    open_until: Option<Instant>,
}

impl HostedMealParser {
    pub(crate) async fn circuit_allows_request(&self) -> bool {
        let mut state = self.circuit.lock().await;
        if state.open_until.is_some_and(|until| until > Instant::now()) {
            return false;
        }
        state.open_until = None;
        true
    }

    pub(crate) async fn record_failure(&self) {
        let mut state = self.circuit.lock().await;
        state.consecutive_failures += 1;
        if state.consecutive_failures >= self.config.circuit_failure_threshold {
            state.open_until = Some(Instant::now() + self.config.circuit_cooldown);
        }
    }

    pub(crate) async fn record_success(&self) {
        *self.circuit.lock().await = CircuitState::default();
    }
}
