use {
    crate::state::State,
    ratatui::{
        backend::CrosstermBackend,
        Terminal,
    },
    std::{
        sync::Arc,
        time::{Duration, Instant},
    },
    tokio::sync::RwLock,
};

/// Run the TUI event loop
/// 
/// Handles keyboard input, terminal resize, and adaptive refresh throttling
pub async fn run_ui(
    state: Arc<RwLock<State>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    let stdout = std::io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Enable raw mode for keyboard input
    crossterm::terminal::enable_raw_mode()?;
    
    // Clear screen and enter alternate screen mode
    // This creates a separate screen buffer, isolating stdout from stderr logs
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide
    )?;
    
    // Clear the terminal
    terminal.clear()?;
    
    // Note: Logs written to stderr will still appear, but alternate screen mode
    // should help isolate them. For complete isolation, we'd need to redirect stderr.
    
    // Track trade rate for adaptive refresh
    let mut last_trade_count = 0;
    let mut last_refresh = Instant::now();
    let mut trade_rate_samples = Vec::new();
    
    loop {
        // Calculate adaptive refresh interval
        let current_trade_count = {
            let state = state.read().await;
            state.total_trade_count()
        };
        
        let trades_since_last = current_trade_count.saturating_sub(last_trade_count);
        let time_since_last = last_refresh.elapsed();
        
        if time_since_last.as_secs_f64() > 0.0 {
            let trades_per_sec = trades_since_last as f64 / time_since_last.as_secs_f64();
            trade_rate_samples.push(trades_per_sec);
            
            // Keep only last 10 samples
            if trade_rate_samples.len() > 10 {
                trade_rate_samples.remove(0);
            }
        }
        
        // Calculate average trade rate
        let avg_trades_per_sec = if trade_rate_samples.is_empty() {
            0.0
        } else {
            trade_rate_samples.iter().sum::<f64>() / trade_rate_samples.len() as f64
        };
        
        // Adaptive throttle: min(1s, 500ms × (avg_trades_per_sec / 10))
        let base_interval = Duration::from_millis(500);
        let throttle_factor = (avg_trades_per_sec / 10.0).max(1.0);
        let refresh_interval = base_interval.mul_f64(throttle_factor).min(Duration::from_secs(1));
        
        // Check for keyboard input (non-blocking)
        if crossterm::event::poll(refresh_interval)? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                match key.code {
                    crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc => {
                        break;
                    }
                    _ => {
                        // Other keys can be handled here (scroll, pause, etc.)
                    }
                }
            }
        }
        
        // Render UI
        {
            let state = state.read().await;
            let area = terminal.size()?;
            terminal.draw(|f| {
                if let Err(e) = crate::ui::layout::render_layout(f, area, &*state) {
                    log::error!("Layout render error: {}", e);
                }
            })?;
        }
        
        last_trade_count = current_trade_count;
        last_refresh = Instant::now();
    }
    
    // Cleanup - restore terminal state
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;
    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}

