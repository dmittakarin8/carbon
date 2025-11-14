//! Rolling window trait definitions and implementations
//!
//! Phase 2: Trait methods and concrete implementations

use super::types::TradeEvent;

/// Trait for managing a single time window (e.g., 60s, 300s, 900s)
///
/// This trait defines the interface for a rolling time window that can:
/// - Accept new trade events
/// - Evict trades older than the window duration
///
/// Phase 2: Trait methods defined and implemented
pub trait RollingWindow {
    /// Add a trade event to this window
    fn add_trade(&mut self, trade: TradeEvent);

    /// Remove trades older than the cutoff timestamp
    fn evict_before(&mut self, cutoff_timestamp: i64);

    /// Check if the window is empty
    fn is_empty(&self) -> bool;

    /// Get the number of trades in this window
    fn len(&self) -> usize;
}

/// Trait for managing multiple rolling windows per token
///
/// This trait coordinates multiple time windows (60s, 300s, 900s) and
/// ensures trades are properly distributed across them.
///
/// Phase 2: Trait methods defined and implemented
pub trait WindowManager {
    /// Update all windows with a new trade event
    fn update(&mut self, trade: TradeEvent);

    /// Clean up old trades from all windows
    fn cleanup(&mut self, now: i64);
}

// Phase 2: Concrete implementations

/// Time window for 60-second rolling period
#[derive(Debug, Clone)]
pub struct TimeWindow60s {
    trades: Vec<TradeEvent>,
    window_duration: i64,
}

impl TimeWindow60s {
    pub fn new() -> Self {
        Self {
            trades: Vec::with_capacity(100),
            window_duration: 60,
        }
    }
}

impl Default for TimeWindow60s {
    fn default() -> Self {
        Self::new()
    }
}

impl RollingWindow for TimeWindow60s {
    fn add_trade(&mut self, trade: TradeEvent) {
        self.trades.push(trade);
    }

    fn evict_before(&mut self, cutoff_timestamp: i64) {
        self.trades.retain(|t| t.timestamp >= cutoff_timestamp);
    }

    fn is_empty(&self) -> bool {
        self.trades.is_empty()
    }

    fn len(&self) -> usize {
        self.trades.len()
    }
}

/// Time window for 300-second (5-minute) rolling period
#[derive(Debug, Clone)]
pub struct TimeWindow300s {
    trades: Vec<TradeEvent>,
    window_duration: i64,
}

impl TimeWindow300s {
    pub fn new() -> Self {
        Self {
            trades: Vec::with_capacity(500),
            window_duration: 300,
        }
    }
}

impl Default for TimeWindow300s {
    fn default() -> Self {
        Self::new()
    }
}

impl RollingWindow for TimeWindow300s {
    fn add_trade(&mut self, trade: TradeEvent) {
        self.trades.push(trade);
    }

    fn evict_before(&mut self, cutoff_timestamp: i64) {
        self.trades.retain(|t| t.timestamp >= cutoff_timestamp);
    }

    fn is_empty(&self) -> bool {
        self.trades.is_empty()
    }

    fn len(&self) -> usize {
        self.trades.len()
    }
}

/// Time window for 900-second (15-minute) rolling period
#[derive(Debug, Clone)]
pub struct TimeWindow900s {
    trades: Vec<TradeEvent>,
    window_duration: i64,
}

impl TimeWindow900s {
    pub fn new() -> Self {
        Self {
            trades: Vec::with_capacity(1500),
            window_duration: 900,
        }
    }
}

impl Default for TimeWindow900s {
    fn default() -> Self {
        Self::new()
    }
}

impl RollingWindow for TimeWindow900s {
    fn add_trade(&mut self, trade: TradeEvent) {
        self.trades.push(trade);
    }

    fn evict_before(&mut self, cutoff_timestamp: i64) {
        self.trades.retain(|t| t.timestamp >= cutoff_timestamp);
    }

    fn is_empty(&self) -> bool {
        self.trades.is_empty()
    }

    fn len(&self) -> usize {
        self.trades.len()
    }
}

/// Multi-window manager coordinating 60s, 300s, and 900s windows
#[derive(Debug, Clone)]
pub struct MultiWindowManager {
    window_60s: TimeWindow60s,
    window_300s: TimeWindow300s,
    window_900s: TimeWindow900s,
}

impl MultiWindowManager {
    pub fn new() -> Self {
        Self {
            window_60s: TimeWindow60s::new(),
            window_300s: TimeWindow300s::new(),
            window_900s: TimeWindow900s::new(),
        }
    }
}

impl Default for MultiWindowManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowManager for MultiWindowManager {
    fn update(&mut self, trade: TradeEvent) {
        // Route trade to all windows
        self.window_60s.add_trade(trade.clone());
        self.window_300s.add_trade(trade.clone());
        self.window_900s.add_trade(trade);
    }

    fn cleanup(&mut self, now: i64) {
        // Evict old trades from each window based on its duration
        self.window_60s.evict_before(now - 60);
        self.window_300s.evict_before(now - 300);
        self.window_900s.evict_before(now - 900);
    }
}

// TODO: Phase 3 - Add metric computation methods to concrete window types
// TODO: Phase 3 - Add window-specific query methods (net_flow, buy_count, etc.)
// TODO: Phase 3 - Integrate with signal detection logic
