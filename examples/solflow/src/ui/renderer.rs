// Renderer module - formatting utilities
// Most rendering logic is in layout.rs, but this module can contain
// additional formatting utilities if needed

/// Format SOL amount for display
pub fn format_sol(amount: f64) -> String {
    format!("{:.6} ◎", amount)
}

/// Format token amount for display
pub fn format_token(amount: f64, decimals: u8) -> String {
    let precision = decimals.min(6) as usize;
    format!("{:.*}", precision, amount)
}

