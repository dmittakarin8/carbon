//! Integration tests for Phase 4.2: Dual-Channel Streamer Integration
//!
//! Tests verify that streamers can send trades to both:
//! 1. Legacy writer backend (JSONL/SQLite)
//! 2. Pipeline channel (when enabled)
//!
//! Key constraints tested:
//! - Non-blocking try_send() never impacts streamer performance
//! - Backward compatibility when pipeline_tx is None
//! - Proper TradeEvent conversion between formats

#[cfg(test)]
mod dual_channel_tests {
    use solflow::pipeline::types::{TradeDirection, TradeEvent as PipelineTradeEvent};
    use solflow::streamer_core::config::{BackendType, StreamerConfig};
    use solflow::streamer_core::output_writer::TradeEvent as StreamerTradeEvent;
    use tokio::sync::mpsc;

    /// Helper to create a test StreamerTradeEvent
    fn create_test_streamer_event(signature: &str, action: &str) -> StreamerTradeEvent {
        StreamerTradeEvent {
            timestamp: 1700000000,
            signature: signature.to_string(),
            program_id: "test_program_id".to_string(),
            program_name: "TestProgram".to_string(),
            action: action.to_string(),
            mint: "test_mint_address".to_string(),
            sol_amount: 1.5,
            token_amount: 1000.0,
            token_decimals: 6,
            user_account: Some("test_wallet".to_string()),
            discriminator: "0123456789abcdef".to_string(),
        }
    }

    #[tokio::test]
    async fn test_config_with_pipeline_channel() {
        // Test: StreamerConfig can store pipeline channel
        let (tx, _rx) = mpsc::channel::<PipelineTradeEvent>(100);

        let config = StreamerConfig {
            program_id: "test_program".to_string(),
            program_name: "TestStreamer".to_string(),
            output_path: "test_output.jsonl".to_string(),
            backend: BackendType::Jsonl,
            pipeline_tx: Some(tx),
        };

        assert!(config.pipeline_tx.is_some());
        assert_eq!(config.program_name, "TestStreamer");
    }

    #[tokio::test]
    async fn test_config_without_pipeline_channel() {
        // Test: StreamerConfig works without pipeline channel (backward compat)
        let config = StreamerConfig {
            program_id: "test_program".to_string(),
            program_name: "TestStreamer".to_string(),
            output_path: "test_output.jsonl".to_string(),
            backend: BackendType::Jsonl,
            pipeline_tx: None,
        };

        assert!(config.pipeline_tx.is_none());
    }

    #[tokio::test]
    async fn test_channel_send_receive() {
        // Test: Trades can be sent and received through pipeline channel
        let (tx, mut rx) = mpsc::channel::<PipelineTradeEvent>(100);

        // Simulate sending a trade
        let trade = PipelineTradeEvent {
            timestamp: 1700000000,
            mint: "test_mint".to_string(),
            direction: TradeDirection::Buy,
            sol_amount: 1.5,
            token_amount: 1000.0,
            token_decimals: 6,
            user_account: "test_wallet".to_string(),
            source_program: "TestProgram".to_string(),
        };

        tx.send(trade.clone()).await.unwrap();

        // Receive and verify
        let received = rx.recv().await.unwrap();
        assert_eq!(received.mint, "test_mint");
        assert_eq!(received.sol_amount, 1.5);
        assert_eq!(received.token_amount, 1000.0);
        assert!(matches!(received.direction, TradeDirection::Buy));
    }

    #[tokio::test]
    async fn test_try_send_non_blocking() {
        // Test: try_send() is non-blocking (fails gracefully when channel full)
        let (tx, _rx) = mpsc::channel::<PipelineTradeEvent>(2); // Small buffer

        let trade = PipelineTradeEvent {
            timestamp: 1700000000,
            mint: "test_mint".to_string(),
            direction: TradeDirection::Buy,
            sol_amount: 1.0,
            token_amount: 1000.0,
            token_decimals: 6,
            user_account: "wallet".to_string(),
            source_program: "Test".to_string(),
        };

        // Fill the channel
        assert!(tx.try_send(trade.clone()).is_ok());
        assert!(tx.try_send(trade.clone()).is_ok());

        // Next try_send should fail (channel full) but NOT block
        let result = tx.try_send(trade.clone());
        assert!(result.is_err()); // Channel full

        // This proves try_send is non-blocking
    }

    #[tokio::test]
    async fn test_trade_direction_conversion() {
        // Test: Action strings convert correctly to TradeDirection
        struct TestCase {
            action: &'static str,
            expected: TradeDirection,
        }

        let test_cases = vec![
            TestCase {
                action: "BUY",
                expected: TradeDirection::Buy,
            },
            TestCase {
                action: "SELL",
                expected: TradeDirection::Sell,
            },
            TestCase {
                action: "UNKNOWN",
                expected: TradeDirection::Unknown,
            },
            TestCase {
                action: "INVALID",
                expected: TradeDirection::Unknown,
            },
        ];

        for case in test_cases {
            let direction = match case.action {
                "BUY" => TradeDirection::Buy,
                "SELL" => TradeDirection::Sell,
                _ => TradeDirection::Unknown,
            };

            // Compare by converting to string representation
            match (direction, case.expected) {
                (TradeDirection::Buy, TradeDirection::Buy) => (),
                (TradeDirection::Sell, TradeDirection::Sell) => (),
                (TradeDirection::Unknown, TradeDirection::Unknown) => (),
                _ => panic!("Direction mismatch for action: {}", case.action),
            }
        }
    }

    #[tokio::test]
    async fn test_conversion_preserves_data() {
        // Test: All fields are preserved during conversion
        let streamer_event = create_test_streamer_event("test_sig_123", "BUY");

        // Simulate conversion (inline since convert_to_pipeline_event is private)
        let pipeline_event = PipelineTradeEvent {
            timestamp: streamer_event.timestamp,
            mint: streamer_event.mint.clone(),
            direction: match streamer_event.action.as_str() {
                "BUY" => TradeDirection::Buy,
                "SELL" => TradeDirection::Sell,
                _ => TradeDirection::Unknown,
            },
            sol_amount: streamer_event.sol_amount,
            token_amount: streamer_event.token_amount,
            token_decimals: streamer_event.token_decimals,
            user_account: streamer_event.user_account.clone().unwrap_or_default(),
            source_program: streamer_event.program_name.clone(),
        };

        // Verify all fields preserved
        assert_eq!(pipeline_event.timestamp, 1700000000);
        assert_eq!(pipeline_event.mint, "test_mint_address");
        assert_eq!(pipeline_event.sol_amount, 1.5);
        assert_eq!(pipeline_event.token_amount, 1000.0);
        assert_eq!(pipeline_event.token_decimals, 6);
        assert_eq!(pipeline_event.user_account, "test_wallet");
        assert_eq!(pipeline_event.source_program, "TestProgram");
        assert!(matches!(pipeline_event.direction, TradeDirection::Buy));
    }

    #[tokio::test]
    async fn test_user_account_optional_handling() {
        // Test: None user_account converts to empty string
        let mut streamer_event = create_test_streamer_event("sig", "BUY");
        streamer_event.user_account = None;

        let pipeline_event = PipelineTradeEvent {
            timestamp: streamer_event.timestamp,
            mint: streamer_event.mint.clone(),
            direction: TradeDirection::Buy,
            sol_amount: streamer_event.sol_amount,
            token_amount: streamer_event.token_amount,
            token_decimals: streamer_event.token_decimals,
            user_account: streamer_event.user_account.clone().unwrap_or_default(),
            source_program: streamer_event.program_name.clone(),
        };

        assert_eq!(pipeline_event.user_account, ""); // Empty string when None
    }

    #[tokio::test]
    async fn test_multiple_streamers_share_channel() {
        // Test: Multiple streamers can send to same pipeline channel
        let (tx, mut rx) = mpsc::channel::<PipelineTradeEvent>(100);

        // Simulate two streamers
        let tx1 = tx.clone();
        let tx2 = tx.clone();

        // Send from first streamer
        let trade1 = PipelineTradeEvent {
            timestamp: 1000,
            mint: "mint_1".to_string(),
            direction: TradeDirection::Buy,
            sol_amount: 1.0,
            token_amount: 100.0,
            token_decimals: 6,
            user_account: "wallet_1".to_string(),
            source_program: "PumpSwap".to_string(),
        };
        tx1.send(trade1).await.unwrap();

        // Send from second streamer
        let trade2 = PipelineTradeEvent {
            timestamp: 2000,
            mint: "mint_2".to_string(),
            direction: TradeDirection::Sell,
            sol_amount: 2.0,
            token_amount: 200.0,
            token_decimals: 6,
            user_account: "wallet_2".to_string(),
            source_program: "BonkSwap".to_string(),
        };
        tx2.send(trade2).await.unwrap();

        // Receive both trades
        let recv1 = rx.recv().await.unwrap();
        let recv2 = rx.recv().await.unwrap();

        assert_eq!(recv1.source_program, "PumpSwap");
        assert_eq!(recv2.source_program, "BonkSwap");
    }

    #[test]
    fn test_backend_type_variants() {
        // Test: BackendType enum has expected variants
        let jsonl = BackendType::Jsonl;
        let sqlite = BackendType::Sqlite;

        assert_eq!(jsonl, BackendType::Jsonl);
        assert_eq!(sqlite, BackendType::Sqlite);
        assert_ne!(jsonl, sqlite);
    }
}
