use crate::streamer_core::config::RuntimeConfig;
use crate::streamer_core::error_handler::{ExponentialBackoff, MaxRetriesExceeded};
use carbon_yellowstone_grpc_datasource::YellowstoneGrpcGeyserClient;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::RwLock;
use yellowstone_grpc_proto::geyser::SubscribeRequestFilterTransactions;

#[derive(Debug)]
pub enum ClientError {
    Connection(String),
    MaxRetries,
}

impl From<MaxRetriesExceeded> for ClientError {
    fn from(_: MaxRetriesExceeded) -> Self {
        ClientError::MaxRetries
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Connection(msg) => write!(f, "Connection error: {}", msg),
            ClientError::MaxRetries => write!(f, "Maximum retry attempts exceeded"),
        }
    }
}

impl std::error::Error for ClientError {}

/// Create gRPC client with multi-program filtering (Option B - APPROVED)
///
/// This function creates a client that subscribes to transactions involving
/// any of the 5 tracked programs: PumpFun, PumpSwap, BonkSwap, Moonshot, Jupiter DCA.
///
/// The gRPC filter matches ANY transaction where these programs appear in the
/// account keys, which covers both outer and inner (CPI) instructions because
/// Solana includes all CPI program IDs in the transaction account list.
pub async fn create_multi_program_client(
    config: &RuntimeConfig,
) -> Result<YellowstoneGrpcGeyserClient, ClientError> {
    let transaction_filter = SubscribeRequestFilterTransactions {
        vote: Some(false),
        failed: Some(false),
        account_include: vec![],
        account_exclude: vec![],
        // CRITICAL: Include all 5 tracked programs
        account_required: vec![
            "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string(), // PumpFun
            "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(), // PumpSwap
            "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj".to_string(), // BonkSwap
            "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG".to_string(),  // Moonshot
            "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M".to_string(), // Jupiter DCA
        ],
        signature: None,
    };

    let mut transaction_filters = HashMap::new();
    transaction_filters.insert("multi_program_filter".to_string(), transaction_filter);

    log::info!("🔗 Creating multi-program gRPC client");
    log::info!("   Filtering: 5 tracked programs (outer + inner instructions)");

    Ok(YellowstoneGrpcGeyserClient::new(
        config.geyser_url.clone(),
        config.x_token.clone(),
        Some(config.commitment_level),
        HashMap::default(),
        transaction_filters,
        Default::default(),
        Arc::new(RwLock::new(HashSet::new())),
        Default::default(),
    ))
}

/// Create gRPC client with single-program filtering (backward compatibility)
///
/// This function is kept for backward compatibility with existing program-specific
/// streamers during the dual-run validation period.
pub async fn create_client(
    config: &RuntimeConfig,
    program_filter: &str,
) -> Result<YellowstoneGrpcGeyserClient, ClientError> {
    let transaction_filter = SubscribeRequestFilterTransactions {
        vote: Some(false),
        failed: Some(false),
        account_include: vec![],
        account_exclude: vec![],
        account_required: vec![program_filter.to_string()],
        signature: None,
    };

    let mut transaction_filters = HashMap::new();
    transaction_filters.insert("program_filter".to_string(), transaction_filter);

    Ok(YellowstoneGrpcGeyserClient::new(
        config.geyser_url.clone(),
        config.x_token.clone(),
        Some(config.commitment_level),
        HashMap::default(),
        transaction_filters,
        Default::default(),
        Arc::new(RwLock::new(HashSet::new())),
        Default::default(),
    ))
}

pub async fn run_with_reconnect<F, Fut>(
    config: &RuntimeConfig,
    program_filter: &str,
    process_fn: F,
) -> Result<(), ClientError>
where
    F: Fn(YellowstoneGrpcGeyserClient) -> Fut,
    Fut: Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>,
{
    let mut backoff = ExponentialBackoff::new(5, 60, 10);

    loop {
        match create_client(config, program_filter).await {
            Ok(client) => {
                log::info!("✅ Connected to gRPC server");
                backoff.reset();
                
                if let Err(e) = process_fn(client).await {
                    log::error!("❌ Pipeline error: {:?}", e);
                    backoff.sleep().await?;
                } else {
                    log::info!("✅ Pipeline completed gracefully");
                    return Ok(());
                }
            }
            Err(e) => {
                log::error!("❌ Connection failed: {:?}", e);
                backoff.sleep().await?;
            }
        }
    }
}
