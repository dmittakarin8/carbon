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
///
/// CRITICAL: Uses OR semantics by creating one filter per program.
/// Multiple filters in the map are treated as OR logic by Yellowstone gRPC.
pub async fn create_multi_program_client(
    config: &RuntimeConfig,
) -> Result<YellowstoneGrpcGeyserClient, ClientError> {
    // Define all tracked programs with their identifiers
    let programs = vec![
        ("pumpfun", "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"),
        ("pumpswap", "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA"),
        ("bonkswap", "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj"),
        ("moonshot", "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG"),
        ("jupiter_dca", "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M"),
    ];

    // Create separate filter for each program (OR logic)
    // account_required with multiple entries uses AND logic (all must be present)
    // Multiple filters in the map use OR logic (any filter can match)
    // This follows the pattern from grpc_verify.rs:486-502
    let mut transaction_filters = HashMap::new();

    for (name, program_id) in programs.iter() {
        let filter = SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            account_include: vec![],
            account_exclude: vec![],
            account_required: vec![program_id.to_string()], // ONE program per filter
            signature: None,
        };
        transaction_filters.insert(format!("{}_filter", name), filter);
    }

    log::info!("🔗 Creating multi-program gRPC client");
    log::info!("   Registered {} transaction filters for multi-program matching", programs.len());
    log::info!("   Filter logic: OR (transactions matching ANY of the 5 programs)");
    log::info!("   Filtering: PumpFun, PumpSwap, BonkSwap, Moonshot, Jupiter DCA");

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
