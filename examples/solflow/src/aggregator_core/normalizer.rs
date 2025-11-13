//! Trade normalization from JSONL events to unified Trade struct

use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub timestamp: i64,
    pub signature: String,
    pub program_name: String,
    pub action: TradeAction,
    pub mint: String,
    pub sol_amount: f64,
    pub token_amount: f64,
    pub token_decimals: u8,
    pub user_account: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeAction {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}

impl Trade {
    /// Parse a Trade from a JSONL line
    pub fn from_jsonl(line: &str) -> Result<Self, Box<dyn Error>> {
        let trade: Trade = serde_json::from_str(line)?;
        Ok(trade)
    }

    /// Check if this trade is a buy
    pub fn is_buy(&self) -> bool {
        matches!(self.action, TradeAction::Buy)
    }

    /// Check if this trade is a sell
    pub fn is_sell(&self) -> bool {
        matches!(self.action, TradeAction::Sell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pumpswap_jsonl() {
        let line = r#"{"timestamp":1763026318,"signature":"7JLwTTCQhtKx8xwjDDkZt6sAeLGNMsJnSmtPfj3cSKHRt8nu5Bnk266NJUgA4TX1dvmChLx9S9CaKXkF3wsinqC","program_id":"pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA","program_name":"PumpSwap","action":"SELL","mint":"CUp8Ve2KroCoTz3jpvLXsAPcMjBjHJquqGJ9n2N4hJZt","sol_amount":7.463367327,"token_amount":1234567.89,"token_decimals":6,"user_account":"EcBxqSKKzWyBLhLLiw9VCrCvd6UwHG9A4TZ1sphpGqxf","discriminator":"0310270000000000"}"#;

        let trade = Trade::from_jsonl(line).unwrap();
        assert_eq!(trade.timestamp, 1763026318);
        assert_eq!(trade.program_name, "PumpSwap");
        assert_eq!(trade.action, TradeAction::Sell);
        assert_eq!(trade.mint, "CUp8Ve2KroCoTz3jpvLXsAPcMjBjHJquqGJ9n2N4hJZt");
        assert_eq!(trade.sol_amount, 7.463367327);
        assert!(trade.is_sell());
    }

    #[test]
    fn test_parse_bonkswap_jsonl() {
        let line = r#"{"timestamp":1763026461,"signature":"5iSSVtkjx62njjAQx2uc1WA3Z9MN69RvcvJ7MQ35FVKjF6NgDBRk8wpq64vqSKkqoih4Ted4ufKYQby1T5ZPn9oN","program_id":"LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj","program_name":"BonkSwap","action":"BUY","mint":"14k8j4vgZBG9FSvHaMsy5UKFdkYgEgFoSLhVdRcHbonk","sol_amount":0.369722109,"token_amount":999999.0,"token_decimals":6,"user_account":"CdpY42BTUgCmvACA8oHeCkvChKHyjqwtRbUAkpSj7xJW","discriminator":"0100000100000000"}"#;

        let trade = Trade::from_jsonl(line).unwrap();
        assert_eq!(trade.program_name, "BonkSwap");
        assert_eq!(trade.action, TradeAction::Buy);
        assert!(trade.is_buy());
    }

    #[test]
    fn test_malformed_jsonl() {
        let line = r#"{"invalid": "json"#;
        assert!(Trade::from_jsonl(line).is_err());
    }
}
