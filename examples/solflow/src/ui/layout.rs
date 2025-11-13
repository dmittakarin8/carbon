use {
    crate::state::State,
    ratatui::{
        layout::{Constraint, Layout as RatLayout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Row, Table},
        Frame,
    },
};

/// Render the main UI layout
pub fn render_layout(f: &mut Frame, area: Rect, state: &State) -> Result<(), Box<dyn std::error::Error>> {
    // Create layout sections
    let chunks = RatLayout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main table
            Constraint::Length(3), // Footer/Status
        ])
        .split(area);
    
    // Render header
    render_header(f, chunks[0]);
    
    // Render main table
    render_trades_table(f, chunks[1], state)?;
    
    // Render footer/status
    render_footer(f, chunks[2], state);
    
    Ok(())
}

fn render_header(f: &mut Frame, area: Rect) {
    let header = Block::default()
        .borders(Borders::ALL)
        .title("Carbon Terminal - Live Trade Monitor");
    
    let text = vec![
        Line::from(vec![
            Span::styled("Carbon Terminal", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" - Live Trade Monitor"),
        ]),
        Line::from(vec![
            Span::raw("Press 'q' or Esc to quit"),
        ]),
    ];
    
    f.render_widget(ratatui::widgets::Paragraph::new(text).block(header), area);
}

fn render_trades_table(f: &mut Frame, area: Rect, state: &State) -> Result<(), Box<dyn std::error::Error>> {
    let trades = state.get_recent_trades();
    
    // Table header
    let header = Row::new(vec![
        "Time",
        "Mint",
        "Direction",
        "SOL Amount",
        "Token Amount",
        "Net Vol",
    ])
    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    
    // Table rows
    let rows: Vec<Row> = trades
        .iter()
        .rev() // Show newest first
        .take(50) // Limit to 50 rows
        .map(|trade| {
            let direction_str = match trade.direction {
                crate::trade_extractor::TradeKind::Buy => "BUY",
                crate::trade_extractor::TradeKind::Sell => "SELL",
                crate::trade_extractor::TradeKind::Unknown => "UNK",
            };
            
            let direction_color = match trade.direction {
                crate::trade_extractor::TradeKind::Buy => Color::Green,
                crate::trade_extractor::TradeKind::Sell => Color::Red,
                crate::trade_extractor::TradeKind::Unknown => Color::Gray,
            };
            
            // Format timestamp
            let timestamp_str = format_timestamp(trade.timestamp);
            
            // Format amounts
            let sol_str = format!("{:.6}", trade.sol_amount);
            let token_str = format!("{:.2}", trade.token_amount);
            
            // Get net volume for this token
            let net_vol = state
                .get_token_metrics(&trade.mint)
                .map(|m| m.buy_volume_sol - m.sell_volume_sol)
                .unwrap_or(0.0);
            let net_vol_str = format!("{:.6}", net_vol);
            
            Row::new(vec![
                timestamp_str,
                trade.mint[..8].to_string(), // First 8 chars of mint
                direction_str.to_string(),
                sol_str,
                token_str,
                net_vol_str,
            ])
            .style(Style::default().fg(direction_color))
        })
        .collect();
    
    let widths = [
        Constraint::Length(12), // Time
        Constraint::Length(10), // Mint
        Constraint::Length(10), // Direction
        Constraint::Length(12), // SOL Amount
        Constraint::Length(15), // Token Amount
        Constraint::Length(12), // Net Vol
    ];
    
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Recent Trades"));
    
    f.render_widget(table, area);
    Ok(())
}

fn render_footer(f: &mut Frame, area: Rect, state: &State) {
    let trade_count = state.total_trade_count();
    let token_count = state.get_all_token_metrics().len();
    
    let text = vec![
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Green)),
            Span::raw("Connected"),
            Span::raw(" | "),
            Span::styled("Trades: ", Style::default().fg(Color::Cyan)),
            Span::raw(trade_count.to_string()),
            Span::raw(" | "),
            Span::styled("Tokens: ", Style::default().fg(Color::Cyan)),
            Span::raw(token_count.to_string()),
        ]),
    ];
    
    let footer = Block::default()
        .borders(Borders::ALL)
        .title("Status");
    
    f.render_widget(ratatui::widgets::Paragraph::new(text).block(footer), area);
}

fn format_timestamp(timestamp: i64) -> String {
    use chrono::DateTime;
    use chrono::Utc;
    
    if let Some(dt) = DateTime::<Utc>::from_timestamp(timestamp, 0) {
        dt.format("%H:%M:%S").to_string()
    } else {
        "N/A".to_string()
    }
}

