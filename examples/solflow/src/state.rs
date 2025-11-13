use {
    crate::aggregator::VolumeAggregator,
    crate::trade_extractor::TradeKind,
    solana_signature::Signature,
    std::{
        collections::HashMap,
        time::{SystemTime, UNIX_EPOCH},
    },
};

/// Represents a single trade extracted from a transaction
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Trade {
    pub signature: Signature,
    pub timestamp: i64,
    pub mint: String,
    pub direction: TradeKind,
    pub sol_amount: f64,
    pub token_amount: f64,
    pub token_decimals: u8,
}

/// Message sent through the channel from processor to state aggregator
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StateMessage {
    Trade(Trade),
    Shutdown,
}

/// In-memory state store for trades and aggregations
/// 
/// Uses channel-based ingestion: processor sends Trade messages via channel,
/// background task receives and aggregates them.
pub struct State {
    /// Recent trades buffer (last N trades for display)
    recent_trades: Vec<Trade>,
    /// Per-token aggregations
    token_metrics: HashMap<String, TokenMetrics>,
    /// Volume aggregator with strict time-cutoff windows
    volume_aggregator: VolumeAggregator,
    /// Maximum number of recent trades to keep
    max_recent_trades: usize,
}

/// Metrics aggregated per token
#[derive(Debug, Clone, Default)]
pub struct TokenMetrics {
    pub total_volume_sol: f64,
    pub buy_volume_sol: f64,
    pub sell_volume_sol: f64,
    pub trade_count: u64,
    pub buy_count: u64,
    pub sell_count: u64,
}

impl State {
    pub fn new(max_recent_trades: usize) -> Self {
        Self {
            recent_trades: Vec::with_capacity(max_recent_trades),
            token_metrics: HashMap::new(),
            volume_aggregator: VolumeAggregator::new(),
            max_recent_trades,
        }
    }

    /// Add a trade to the state (called by background aggregator task)
    pub fn add_trade(&mut self, trade: Trade) {
        // Add to recent trades buffer
        self.recent_trades.push(trade.clone());
        
        // Maintain buffer size
        if self.recent_trades.len() > self.max_recent_trades {
            self.recent_trades.remove(0);
        }

        // Update token metrics
        let metrics = self.token_metrics.entry(trade.mint.clone()).or_default();
        metrics.total_volume_sol += trade.sol_amount;
        metrics.trade_count += 1;

        match trade.direction {
            TradeKind::Buy => {
                metrics.buy_volume_sol += trade.sol_amount;
                metrics.buy_count += 1;
            }
            TradeKind::Sell => {
                metrics.sell_volume_sol += trade.sol_amount;
                metrics.sell_count += 1;
            }
            TradeKind::Unknown => {
                // Count but don't add to buy/sell volumes
            }
        }
        
        // Add to volume aggregator (strict time-cutoff windows)
        self.volume_aggregator.add_trade(trade);
    }

    /// Get recent trades for display
    pub fn get_recent_trades(&self) -> &[Trade] {
        &self.recent_trades
    }

    /// Get metrics for a specific token
    pub fn get_token_metrics(&self, mint: &str) -> Option<&TokenMetrics> {
        self.token_metrics.get(mint)
    }

    /// Get all token metrics
    pub fn get_all_token_metrics(&self) -> &HashMap<String, TokenMetrics> {
        &self.token_metrics
    }

    /// Get total trade count
    pub fn total_trade_count(&self) -> usize {
        self.recent_trades.len()
    }
    
    /// Get net volume for a token (from aggregator)
    #[allow(dead_code)]
    pub fn get_net_volume(&self, mint: &str) -> f64 {
        self.volume_aggregator.get_net_volume(mint)
    }
    
    /// Get volume for 1-minute window
    #[allow(dead_code)]
    pub fn get_volume_1m(&self, mint: &str) -> f64 {
        self.volume_aggregator.get_volume_1m(mint)
    }
    
    /// Get volume for 5-minute window
    #[allow(dead_code)]
    pub fn get_volume_5m(&self, mint: &str) -> f64 {
        self.volume_aggregator.get_volume_5m(mint)
    }
    
    /// Get volume for 15-minute window
    #[allow(dead_code)]
    pub fn get_volume_15m(&self, mint: &str) -> f64 {
        self.volume_aggregator.get_volume_15m(mint)
    }
}

/// Background task that receives trades from channel and aggregates them into State
pub async fn state_aggregator_task(
    mut receiver: tokio::sync::mpsc::Receiver<StateMessage>,
    state: std::sync::Arc<tokio::sync::RwLock<State>>,
) {
    log::info!("State aggregator task started");
    
    while let Some(message) = receiver.recv().await {
        match message {
            StateMessage::Trade(trade) => {
                let mut state = state.write().await;
                state.add_trade(trade);
            }
            StateMessage::Shutdown => {
                log::info!("State aggregator received shutdown signal");
                break;
            }
        }
    }
    
    log::info!("State aggregator task stopped");
}

/// Helper to get current Unix timestamp
pub fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

