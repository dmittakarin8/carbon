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
