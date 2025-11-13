use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug)]
pub struct ExponentialBackoff {
    initial_delay: u64,
    max_delay: u64,
    max_retries: u32,
    current_attempt: u32,
}

#[derive(Debug)]
pub struct MaxRetriesExceeded;

impl std::fmt::Display for MaxRetriesExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Maximum retry attempts exceeded")
    }
}

impl std::error::Error for MaxRetriesExceeded {}

impl ExponentialBackoff {
    pub fn new(initial: u64, max: u64, retries: u32) -> Self {
        Self {
            initial_delay: initial,
            max_delay: max,
            max_retries: retries,
            current_attempt: 0,
        }
    }

    pub async fn sleep(&mut self) -> Result<(), MaxRetriesExceeded> {
        if self.current_attempt >= self.max_retries {
            return Err(MaxRetriesExceeded);
        }

        let delay = std::cmp::min(
            self.initial_delay * 2_u64.pow(self.current_attempt),
            self.max_delay,
        );

        log::warn!(
            "⏳ Retry attempt {} of {} in {}s",
            self.current_attempt + 1,
            self.max_retries,
            delay
        );

        sleep(Duration::from_secs(delay)).await;
        self.current_attempt += 1;
        Ok(())
    }

    pub fn reset(&mut self) {
        self.current_attempt = 0;
    }
}
