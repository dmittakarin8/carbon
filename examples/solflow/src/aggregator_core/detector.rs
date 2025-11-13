//! Signal detection with configurable thresholds

pub struct SignalDetector {
    uptrend_threshold: f64,
    accumulation_threshold: f64,
}

impl SignalDetector {
    pub fn new(uptrend_threshold: f64, accumulation_threshold: f64) -> Self {
        Self {
            uptrend_threshold,
            accumulation_threshold,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(0.7, 25.0)
    }

    /// Detect signals based on uptrend score, DCA overlap, and net flow
    ///
    /// # Signals
    /// - **UPTREND**: High uptrend_score (> threshold)
    /// - **ACCUMULATION**: High DCA overlap AND positive net flow
    ///
    /// # Priority
    /// ACCUMULATION takes precedence over UPTREND if both conditions are met
    pub fn detect_signals(
        &self,
        uptrend_score: f64,
        dca_overlap_pct: f64,
        net_flow_sol: f64,
    ) -> Option<String> {
        // ACCUMULATION signal (higher priority)
        if dca_overlap_pct > self.accumulation_threshold && net_flow_sol > 0.0 {
            return Some("ACCUMULATION".to_string());
        }

        // UPTREND signal
        if uptrend_score > self.uptrend_threshold {
            return Some("UPTREND".to_string());
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accumulation_signal() {
        let detector = SignalDetector::with_defaults();

        let signal = detector.detect_signals(0.6, 30.0, 100.0);
        assert_eq!(signal, Some("ACCUMULATION".to_string()));
    }

    #[test]
    fn test_uptrend_signal() {
        let detector = SignalDetector::with_defaults();

        let signal = detector.detect_signals(0.8, 10.0, 50.0);
        assert_eq!(signal, Some("UPTREND".to_string()));
    }

    #[test]
    fn test_no_signal() {
        let detector = SignalDetector::with_defaults();

        let signal = detector.detect_signals(0.5, 10.0, 50.0);
        assert_eq!(signal, None);
    }

    #[test]
    fn test_accumulation_takes_precedence() {
        let detector = SignalDetector::with_defaults();

        // Both conditions met, ACCUMULATION should be returned
        let signal = detector.detect_signals(0.8, 30.0, 100.0);
        assert_eq!(signal, Some("ACCUMULATION".to_string()));
    }

    #[test]
    fn test_negative_flow_blocks_accumulation() {
        let detector = SignalDetector::with_defaults();

        // High DCA overlap but negative flow -> no ACCUMULATION
        let signal = detector.detect_signals(0.5, 30.0, -50.0);
        assert_eq!(signal, None);
    }
}
