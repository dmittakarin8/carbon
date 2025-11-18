//! DexScreener API Integration
//!
//! Provides token metadata enrichment from DexScreener API including:
//! - Token name and symbol
//! - Current price (USD)
//! - Market capitalization
//! - Token image URL
//!
//! ## API Reference
//!
//! Endpoint: https://api.dexscreener.com/token-pairs/v1/solana/{mint}
//! Returns: Array of trading pairs for the token
//!
//! ## Usage
//!
//! ```rust
//! use solflow::pipeline::dexscreener;
//!
//! let metadata = dexscreener::fetch_token_metadata("MINT_ADDRESS").await?;
//! dexscreener::upsert_metadata(&conn, &metadata).await?;
//! ```

use reqwest;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// DexScreener pair response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexScreenerPair {
    #[serde(rename = "baseToken")]
    pub base_token: BaseToken,
    #[serde(rename = "quoteToken")]
    pub quote_token: QuoteToken,
    #[serde(rename = "priceUsd")]
    pub price_usd: String,
    #[serde(rename = "marketCap")]
    pub market_cap: Option<f64>,
    pub info: Option<PairInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseToken {
    pub name: String,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteToken {
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairInfo {
    #[serde(rename = "imageUrl")]
    pub image_url: Option<String>,
}

/// Token metadata extracted from DexScreener
#[derive(Debug, Clone)]
pub struct TokenMetadata {
    pub mint: String,
    pub name: String,
    pub symbol: String,
    pub image_url: Option<String>,
    pub price_usd: f64,
    pub market_cap: Option<f64>,
}

/// Fetch token metadata from DexScreener API
///
/// Returns the first pair where quoteToken.symbol == "SOL".
///
/// # Arguments
/// * `mint` - Token mint address
///
/// # Returns
/// * `Ok(TokenMetadata)` - Successfully fetched metadata
/// * `Err(...)` - API error or no SOL pair found
///
/// # Example
/// ```rust
/// let metadata = fetch_token_metadata("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").await?;
/// println!("Token: {} ({})", metadata.name, metadata.symbol);
/// ```
pub async fn fetch_token_metadata(mint: &str) -> Result<TokenMetadata, Box<dyn std::error::Error>> {
    let url = format!("https://api.dexscreener.com/token-pairs/v1/solana/{}", mint);
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    
    let response = client.get(&url).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("DexScreener API error: {}", response.status()).into());
    }
    
    let pairs: Vec<DexScreenerPair> = response.json().await?;
    
    // Find first pair with SOL quote token
    let pair = pairs.iter()
        .find(|p| p.quote_token.symbol == "SOL")
        .ok_or("No SOL pair found")?;
    
    Ok(TokenMetadata {
        mint: mint.to_string(),
        name: pair.base_token.name.clone(),
        symbol: pair.base_token.symbol.clone(),
        image_url: pair.info.as_ref().and_then(|i| i.image_url.clone()),
        price_usd: pair.price_usd.parse().unwrap_or(0.0),
        market_cap: pair.market_cap,
    })
}

/// Upsert metadata into token_metadata table
///
/// Updates existing row or inserts new one. Preserves existing values
/// for decimals, blocked, and follow_price flags.
///
/// # Arguments
/// * `conn` - SQLite connection
/// * `metadata` - Token metadata to upsert
///
/// # Returns
/// * `Ok(())` - Successfully upserted
/// * `Err(...)` - Database error
///
/// # Example
/// ```rust
/// let conn = Connection::open("solflow.db")?;
/// upsert_metadata(&conn, &metadata)?;
/// ```
pub fn upsert_metadata(
    conn: &Connection,
    metadata: &TokenMetadata,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now().timestamp();
    
    conn.execute(
        r#"
        INSERT INTO token_metadata 
            (mint, name, symbol, image_url, price_usd, market_cap, updated_at, created_at, decimals, blocked, follow_price)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 0, 0, 0)
        ON CONFLICT(mint) DO UPDATE SET
            name = excluded.name,
            symbol = excluded.symbol,
            image_url = excluded.image_url,
            price_usd = excluded.price_usd,
            market_cap = excluded.market_cap,
            updated_at = excluded.updated_at
        "#,
        rusqlite::params![
            metadata.mint,
            metadata.name,
            metadata.symbol,
            metadata.image_url,
            metadata.price_usd,
            metadata.market_cap,
            now,
        ],
    )?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Run only when testing with live API
    async fn test_fetch_token_metadata() {
        // USDC mint address (known to exist)
        let mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        
        let result = fetch_token_metadata(mint).await;
        assert!(result.is_ok());
        
        let metadata = result.unwrap();
        assert_eq!(metadata.mint, mint);
        assert!(!metadata.name.is_empty());
        assert!(!metadata.symbol.is_empty());
    }
}
