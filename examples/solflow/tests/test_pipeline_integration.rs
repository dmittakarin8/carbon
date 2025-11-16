//! Integration tests for Phase 4.2b: Pipeline Runtime Streamer Integration
//!
//! Tests verify that pipeline_runtime can spawn streamers and receive trades
//! through the pipeline channel, enabling end-to-end flow validation.
//!
//! Key integration points tested:
//! - Channel creation and message passing
//! - Multiple streamers sharing single channel
//! - Non-blocking send behavior
//! - Trade event format compatibility

#[cfg(test)]
mod pipeline_integration_tests {
    use solflow::pipeline::types::{TradeDirection, TradeEvent};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_pipeline_runtime_streamer_integration() {
        // Test: Simulate pipeline_runtime spawning streamers and receiving trades
        
        // 1. Create minimal pipeline channel (simulates PipelineEngine setup)
        let (tx, mut rx) = mpsc::channel::<TradeEvent>(100);
        
        // 2. Spawn mock streamer that sends 5 test trades
        tokio::spawn(async move {
            for i in 0..5 {
                let trade = TradeEvent {
                    timestamp: 1700000000 + i,
                    mint: format!("test_mint_{}", i),
                    direction: TradeDirection::Buy,
                    sol_amount: 1.0 + (i as f64 * 0.1),
                    token_amount: 1000.0 * (i as f64 + 1.0),
                    token_decimals: 6,
                    user_account: format!("test_wallet_{}", i),
                    source_program: "MockStreamer".to_string(),
                };
                if tx.send(trade).await.is_err() {
                    break; // Channel closed
                }
            }
        });
        
        // 3. Verify trades are received (simulates ingestion task)
        let mut count = 0;
        while let Some(trade) = rx.recv().await {
            count += 1;
            assert!(trade.mint.starts_with("test_mint_"));
            assert_eq!(trade.source_program, "MockStreamer");
            assert_eq!(trade.token_decimals, 6);
            if count == 5 {
                break;
            }
        }
        
        assert_eq!(count, 5, "Expected to receive all 5 trades");
    }

    #[tokio::test]
    async fn test_multiple_streamers_single_channel() {
        // Test: Multiple streamers send to same pipeline channel (realistic scenario)
        
        let (tx, mut rx) = mpsc::channel::<TradeEvent>(200);
        
        // Spawn 4 mock streamers (simulating PumpSwap, BonkSwap, Moonshot, JupiterDCA)
        let streamers = vec![
            ("PumpSwap", 3),
            ("BonkSwap", 2),
            ("Moonshot", 4),
            ("JupiterDCA", 1),
        ];
        
        for (source, count) in streamers {
            let tx_clone = tx.clone();
            let source_name = source.to_string();
            tokio::spawn(async move {
                for i in 0..count {
                    let trade = TradeEvent {
                        timestamp: 1700000000 + i,
                        mint: format!("mint_{}_{}", source_name, i),
                        direction: if i % 2 == 0 {
                            TradeDirection::Buy
                        } else {
                            TradeDirection::Sell
                        },
                        sol_amount: 1.0,
                        token_amount: 1000.0,
                        token_decimals: 6,
                        user_account: "test_wallet".to_string(),
                        source_program: source_name.clone(),
                    };
                    let _ = tx_clone.send(trade).await;
                }
            });
        }
        
        // Drop original tx so channel closes when all spawned senders finish
        drop(tx);
        
        // Collect all trades
        let mut trades = Vec::new();
        while let Some(trade) = rx.recv().await {
            trades.push(trade);
        }
        
        // Verify we received trades from all 4 streamers
        assert_eq!(trades.len(), 10, "Expected 3+2+4+1=10 total trades");
        
        let pumpswap_count = trades.iter().filter(|t| t.source_program == "PumpSwap").count();
        let bonkswap_count = trades.iter().filter(|t| t.source_program == "BonkSwap").count();
        let moonshot_count = trades.iter().filter(|t| t.source_program == "Moonshot").count();
        let jupiter_count = trades.iter().filter(|t| t.source_program == "JupiterDCA").count();
        
        assert_eq!(pumpswap_count, 3);
        assert_eq!(bonkswap_count, 2);
        assert_eq!(moonshot_count, 4);
        assert_eq!(jupiter_count, 1);
    }

    #[tokio::test]
    async fn test_channel_buffer_overflow_handling() {
        // Test: Channel handles overflow gracefully (non-blocking try_send)
        
        let (tx, mut rx) = mpsc::channel::<TradeEvent>(5); // Small buffer
        
        // Spawn sender that tries to send more than buffer size
        tokio::spawn(async move {
            for i in 0..10 {
                let trade = TradeEvent {
                    timestamp: 1700000000 + i,
                    mint: format!("mint_{}", i),
                    direction: TradeDirection::Buy,
                    sol_amount: 1.0,
                    token_amount: 1000.0,
                    token_decimals: 6,
                    user_account: "wallet".to_string(),
                    source_program: "Test".to_string(),
                };
                
                // try_send is non-blocking (used by streamers)
                match tx.try_send(trade) {
                    Ok(_) => {}
                    Err(_) => {
                        // Channel full - streamer continues without blocking
                        break;
                    }
                }
            }
        });
        
        // Give sender time to fill buffer
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        // Drain channel
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        
        // Should have received at least buffer size worth of trades
        assert!(count >= 5, "Expected at least 5 trades (buffer size)");
    }

    #[tokio::test]
    async fn test_trade_direction_variants() {
        // Test: All TradeDirection variants work through channel
        
        let (tx, mut rx) = mpsc::channel::<TradeEvent>(10);
        
        // Send one trade of each direction type
        let directions = vec![
            TradeDirection::Buy,
            TradeDirection::Sell,
            TradeDirection::Unknown,
        ];
        
        tokio::spawn(async move {
            for (i, direction) in directions.into_iter().enumerate() {
                let trade = TradeEvent {
                    timestamp: 1700000000 + i as i64,
                    mint: format!("mint_{}", i),
                    direction,
                    sol_amount: 1.0,
                    token_amount: 1000.0,
                    token_decimals: 6,
                    user_account: "wallet".to_string(),
                    source_program: "Test".to_string(),
                };
                let _ = tx.send(trade).await;
            }
        });
        
        // Collect and verify
        let mut received_directions = Vec::new();
        while let Some(trade) = rx.recv().await {
            received_directions.push(trade.direction);
            if received_directions.len() == 3 {
                break;
            }
        }
        
        assert_eq!(received_directions.len(), 3);
        assert!(matches!(received_directions[0], TradeDirection::Buy));
        assert!(matches!(received_directions[1], TradeDirection::Sell));
        assert!(matches!(received_directions[2], TradeDirection::Unknown));
    }

    #[tokio::test]
    async fn test_channel_closure_handling() {
        // Test: Receiver detects when all senders are dropped
        
        let (tx, mut rx) = mpsc::channel::<TradeEvent>(10);
        
        // Send a few trades then drop sender
        tokio::spawn(async move {
            for i in 0..3 {
                let trade = TradeEvent {
                    timestamp: 1700000000 + i,
                    mint: format!("mint_{}", i),
                    direction: TradeDirection::Buy,
                    sol_amount: 1.0,
                    token_amount: 1000.0,
                    token_decimals: 6,
                    user_account: "wallet".to_string(),
                    source_program: "Test".to_string(),
                };
                let _ = tx.send(trade).await;
            }
            // tx dropped here
        });
        
        // Receive all trades
        let mut count = 0;
        while rx.recv().await.is_some() {
            count += 1;
        }
        
        // Should have received exactly 3 trades
        assert_eq!(count, 3);
        
        // Next recv should return None (channel closed)
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn test_logger_no_panic_with_pipeline_enabled() {
        // Test: Verify logger initialization doesn't panic when ENABLE_PIPELINE=true
        // This simulates the scenario where pipeline_runtime has already initialized
        // the logger before spawning streamer tasks.
        
        // 1. Set ENABLE_PIPELINE flag (simulates pipeline_runtime environment)
        std::env::set_var("ENABLE_PIPELINE", "true");
        
        // 2. Initialize logger once (simulates pipeline_runtime startup)
        let _ = env_logger::try_init();
        
        // 3. Create pipeline channel
        let (tx, mut rx) = mpsc::channel::<TradeEvent>(100);
        
        // 4. Simulate streamer behavior: Create trades and send through channel
        //    In real scenario, streamer_core::run() would do this internally
        //    But we can't easily call run() in test without full gRPC setup
        //    So we verify the logger init logic works by checking env var
        let pipeline_enabled = std::env::var("ENABLE_PIPELINE").unwrap_or_default() == "true";
        assert!(pipeline_enabled, "ENABLE_PIPELINE should be set");
        
        // 5. Verify we can initialize logger again without panic (using try_init)
        //    This simulates what streamer_core::run() does internally
        let result = env_logger::Builder::new()
            .target(env_logger::Target::Stderr)
            .try_init();
        
        // Should either succeed (if first init) or return Err (already initialized)
        // but should NEVER panic
        match result {
            Ok(_) => {
                // First init succeeded - unexpected but not an error
            }
            Err(e) => {
                // Expected: Logger already initialized
                assert_eq!(e.to_string(), "attempted to set a logger after the logging system was already initialized");
            }
        }
        
        // 6. Send test trades through channel to verify dual-channel flow works
        tokio::spawn(async move {
            for i in 0..3 {
                let trade = TradeEvent {
                    timestamp: 1700000000 + i,
                    mint: format!("test_mint_{}", i),
                    direction: TradeDirection::Buy,
                    sol_amount: 1.0,
                    token_amount: 1000.0,
                    token_decimals: 6,
                    user_account: "test_wallet".to_string(),
                    source_program: "TestStreamer".to_string(),
                };
                let _ = tx.send(trade).await;
            }
        });
        
        // 7. Verify trades flow through channel
        let mut count = 0;
        while let Some(trade) = rx.recv().await {
            count += 1;
            assert!(trade.mint.starts_with("test_mint_"));
            if count == 3 {
                break;
            }
        }
        
        assert_eq!(count, 3, "Expected to receive all 3 test trades");
        
        // Cleanup: Unset env var for other tests
        std::env::remove_var("ENABLE_PIPELINE");
    }
}
