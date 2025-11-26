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
    #[serde(rename = "pairCreatedAt")]
    pub pair_created_at: Option<i64>,
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
    pub pair_created_at: Option<i64>,
}

/// Token price data extracted from DexScreener (price-only, no metadata)
///
/// This struct is used for backend price monitoring where only market data
/// is needed, without fetching metadata fields like name, symbol, or image.
#[derive(Debug, Clone)]
pub struct TokenPrice {
    pub mint: String,
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
        // Convert pairCreatedAt from milliseconds to seconds for consistency with other timestamps
        pair_created_at: pair.pair_created_at.map(|ms| ms / 1000),
    })
}

/// Fetch token price from DexScreener API (price-only, no metadata)
///
/// Returns only price and market cap data, without fetching metadata fields.
/// This is used by backend price monitoring to avoid duplicate metadata fetches.
///
/// Uses flexible JSON parsing to handle heterogeneous DexScreener responses where
/// pairs may have missing or malformed fields. Filters for SOL pairs and selects
/// the one with highest liquidity when multiple valid pairs exist.
///
/// # Arguments
/// * `mint` - Token mint address
///
/// # Returns
/// * `Ok(TokenPrice)` - Successfully fetched price data
/// * `Err(...)` - API error or no SOL pair found
///
/// # Example
/// ```rust
/// let price = fetch_token_price("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").await?;
/// println!("Price: ${}", price.price_usd);
/// ```
pub async fn fetch_token_price(mint: &str) -> Result<TokenPrice, Box<dyn std::error::Error>> {
    let url = format!("https://api.dexscreener.com/token-pairs/v1/solana/{}", mint);
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    
    let response = client.get(&url).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("DexScreener API error: {}", response.status()).into());
    }
    
    // Parse response as flexible JSON to handle heterogeneous pair data
    let json: serde_json::Value = response.json().await?;
    let pairs = json.as_array()
        .ok_or("Response is not an array")?;
    
    // Collect valid SOL pairs with their liquidity for ranking
    let mut valid_sol_pairs: Vec<(f64, Option<f64>, Option<f64>)> = Vec::new();
    
    for pair in pairs {
        // Skip pairs without SOL quote token
        let quote_symbol = pair.get("quoteToken")
            .and_then(|qt| qt.get("symbol"))
            .and_then(|s| s.as_str());
        
        if quote_symbol != Some("SOL") {
            continue;
        }
        
        // Extract priceUsd (required field)
        let price_usd = match pair.get("priceUsd")
            .and_then(|p| p.as_str())
            .and_then(|s| s.parse::<f64>().ok())
        {
            Some(p) if p > 0.0 => p,
            _ => continue, // Skip pairs without valid price
        };
        
        // Extract marketCap (optional)
        let market_cap = pair.get("marketCap")
            .and_then(|mc| mc.as_f64());
        
        // Extract liquidity.usd (optional, used for ranking)
        let liquidity = pair.get("liquidity")
            .and_then(|l| l.get("usd"))
            .and_then(|u| u.as_f64());
        
        valid_sol_pairs.push((price_usd, market_cap, liquidity));
    }
    
    // Select best pair: highest liquidity, or first if liquidity missing
    let best_pair = valid_sol_pairs.into_iter()
        .max_by(|a, b| {
            match (a.2, b.2) {
                (Some(liq_a), Some(liq_b)) => liq_a.partial_cmp(&liq_b).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => std::cmp::Ordering::Equal,
            }
        })
        .ok_or("No valid SOL pair found with price data")?;
    
    Ok(TokenPrice {
        mint: mint.to_string(),
        price_usd: best_pair.0,
        market_cap: best_pair.1,
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
            (mint, name, symbol, image_url, price_usd, market_cap, pair_created_at, updated_at, created_at, decimals, blocked, follow_price)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 0, 0, 0)
        ON CONFLICT(mint) DO UPDATE SET
            name = excluded.name,
            symbol = excluded.symbol,
            image_url = excluded.image_url,
            price_usd = excluded.price_usd,
            market_cap = excluded.market_cap,
            pair_created_at = COALESCE(token_metadata.pair_created_at, excluded.pair_created_at),
            updated_at = excluded.updated_at
        "#,
        rusqlite::params![
            metadata.mint,
            metadata.name,
            metadata.symbol,
            metadata.image_url,
            metadata.price_usd,
            metadata.market_cap,
            metadata.pair_created_at,
            now,
        ],
    )?;
    
    Ok(())
}

/// Update price data in token_metadata table (price-only, no metadata)
///
/// Updates only price_usd, market_cap, and updated_at fields.
/// Does NOT modify metadata fields like name, symbol, or image_url.
/// Requires row to already exist (created by frontend).
///
/// # Arguments
/// * `conn` - SQLite connection
/// * `price` - Token price data to update
///
/// # Returns
/// * `Ok(())` - Successfully updated
/// * `Err(...)` - Database error
///
/// # Example
/// ```rust
/// let conn = Connection::open("solflow.db")?;
/// upsert_price(&conn, &price)?;
/// ```
pub fn upsert_price(
    conn: &Connection,
    price: &TokenPrice,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now().timestamp();
    
    conn.execute(
        r#"
        UPDATE token_metadata
        SET 
            price_usd = ?,
            market_cap = ?,
            updated_at = ?
        WHERE mint = ?
        "#,
        rusqlite::params![
            price.price_usd,
            price.market_cap,
            now,
            price.mint,
        ],
    )?;
    
    Ok(())
}

/// Check if a token row exists in token_metadata table with follow_price = 1
///
/// Used by backend to validate that a row exists before attempting price updates.
/// Prevents errors when trying to update non-existent or invalid mints.
///
/// # Arguments
/// * `conn` - SQLite connection
/// * `mint` - Token mint address to check
///
/// # Returns
/// * `true` - Row exists and follow_price = 1
/// * `false` - Row doesn't exist or follow_price = 0
///
/// # Example
/// ```rust
/// if row_exists(&conn, &mint) {
///     // Safe to update price
/// }
/// ```
pub fn row_exists(conn: &Connection, mint: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM token_metadata WHERE mint = ? AND follow_price = 1 LIMIT 1",
        rusqlite::params![mint],
        |_| Ok(()),
    )
    .is_ok()
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

    #[test]
    fn test_tolerant_parsing_mixed_pairs() {
        // Simulate heterogeneous DexScreener response with:
        // - Malformed pair (missing priceUsd)
        // - USDC pair (should be skipped)
        // - Two SOL pairs with different liquidity
        let json_response = r#"[
            {
                "baseToken": {"name": "Test", "symbol": "TEST"},
                "quoteToken": {"symbol": "SOL"},
                "marketCap": 100000
            },
            {
                "baseToken": {"name": "Test", "symbol": "TEST"},
                "quoteToken": {"symbol": "USDC"},
                "priceUsd": "1.50",
                "marketCap": 200000,
                "liquidity": {"usd": 50000}
            },
            {
                "baseToken": {"name": "Test", "symbol": "TEST"},
                "quoteToken": {"symbol": "SOL"},
                "priceUsd": "2.00",
                "marketCap": 300000,
                "liquidity": {"usd": 10000}
            },
            {
                "baseToken": {"name": "Test", "symbol": "TEST"},
                "quoteToken": {"symbol": "SOL"},
                "priceUsd": "2.10",
                "marketCap": 350000,
                "liquidity": {"usd": 75000}
            }
        ]"#;

        let json: serde_json::Value = serde_json::from_str(json_response).unwrap();
        let pairs = json.as_array().unwrap();

        // Simulate the logic from fetch_token_price
        let mut valid_sol_pairs: Vec<(f64, Option<f64>, Option<f64>)> = Vec::new();

        for pair in pairs {
            let quote_symbol = pair.get("quoteToken")
                .and_then(|qt| qt.get("symbol"))
                .and_then(|s| s.as_str());

            if quote_symbol != Some("SOL") {
                continue;
            }

            let price_usd = match pair.get("priceUsd")
                .and_then(|p| p.as_str())
                .and_then(|s| s.parse::<f64>().ok())
            {
                Some(p) if p > 0.0 => p,
                _ => continue,
            };

            let market_cap = pair.get("marketCap").and_then(|mc| mc.as_f64());
            let liquidity = pair.get("liquidity")
                .and_then(|l| l.get("usd"))
                .and_then(|u| u.as_f64());

            valid_sol_pairs.push((price_usd, market_cap, liquidity));
        }

        // Should have found 2 valid SOL pairs (skipped malformed and USDC)
        assert_eq!(valid_sol_pairs.len(), 2);

        // Select best pair by liquidity
        let best_pair = valid_sol_pairs.into_iter()
            .max_by(|a, b| {
                match (a.2, b.2) {
                    (Some(liq_a), Some(liq_b)) => liq_a.partial_cmp(&liq_b).unwrap_or(std::cmp::Ordering::Equal),
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            })
            .unwrap();

        // Should select the pair with highest liquidity (75000)
        assert_eq!(best_pair.0, 2.10); // price_usd
        assert_eq!(best_pair.1, Some(350000.0)); // market_cap
        assert_eq!(best_pair.2, Some(75000.0)); // liquidity
    }

    #[test]
    fn test_tolerant_parsing_no_liquidity() {
        // Test case where SOL pairs have no liquidity data
        let json_response = r#"[
            {
                "baseToken": {"name": "Test", "symbol": "TEST"},
                "quoteToken": {"symbol": "SOL"},
                "priceUsd": "1.50",
                "marketCap": 100000
            },
            {
                "baseToken": {"name": "Test", "symbol": "TEST"},
                "quoteToken": {"symbol": "SOL"},
                "priceUsd": "1.55",
                "marketCap": 110000
            }
        ]"#;

        let json: serde_json::Value = serde_json::from_str(json_response).unwrap();
        let pairs = json.as_array().unwrap();

        let mut valid_sol_pairs: Vec<(f64, Option<f64>, Option<f64>)> = Vec::new();

        for pair in pairs {
            let quote_symbol = pair.get("quoteToken")
                .and_then(|qt| qt.get("symbol"))
                .and_then(|s| s.as_str());

            if quote_symbol != Some("SOL") {
                continue;
            }

            let price_usd = match pair.get("priceUsd")
                .and_then(|p| p.as_str())
                .and_then(|s| s.parse::<f64>().ok())
            {
                Some(p) if p > 0.0 => p,
                _ => continue,
            };

            let market_cap = pair.get("marketCap").and_then(|mc| mc.as_f64());
            let liquidity = pair.get("liquidity")
                .and_then(|l| l.get("usd"))
                .and_then(|u| u.as_f64());

            valid_sol_pairs.push((price_usd, market_cap, liquidity));
        }

        assert_eq!(valid_sol_pairs.len(), 2);

        // When no liquidity, max_by returns last pair when all equal
        let best_pair = valid_sol_pairs.into_iter()
            .max_by(|a, b| {
                match (a.2, b.2) {
                    (Some(liq_a), Some(liq_b)) => liq_a.partial_cmp(&liq_b).unwrap_or(std::cmp::Ordering::Equal),
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            })
            .unwrap();

        // When liquidity is equal (both None), max_by returns last element
        assert_eq!(best_pair.0, 1.55);
        assert_eq!(best_pair.1, Some(110000.0));
    }
}
