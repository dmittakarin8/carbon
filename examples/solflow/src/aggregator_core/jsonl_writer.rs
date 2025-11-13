//! JSONL writer for enriched metrics - outputs aggregated signals to per-window JSONL files

use super::window::WindowSize;
use super::writer_backend::{AggregatorWriterBackend, AggregatorWriterError};
use async_trait::async_trait;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;
use std::time::Duration;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct EnrichedMetrics {
    pub mint: String,
    pub window: String,
    pub net_flow_sol: f64,
    pub buy_sell_ratio: f64,
    pub dca_overlap_pct: f64,
    pub uptrend_score: f64,
    pub signal: Option<String>,
    pub timestamp: i64,
}

pub struct EnrichedMetricsWriter {
    writers: HashMap<WindowSize, BufWriter<std::fs::File>>,
    last_flush: Instant,
}

impl EnrichedMetricsWriter {
    pub fn new(base_path: PathBuf) -> std::io::Result<Self> {
        let mut writers = HashMap::new();

        for window in WindowSize::all() {
            let filename = format!("{}.jsonl", window.as_str());
            let file_path = base_path.join(filename);

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)?;

            log::info!("📝 Writing enriched metrics to: {}", file_path.display());
            writers.insert(window, BufWriter::new(file));
        }

        Ok(Self {
            writers,
            last_flush: Instant::now(),
        })
    }

    pub fn write_metrics(&mut self, metrics: &EnrichedMetrics) -> std::io::Result<()> {
        let window = WindowSize::from_str(&metrics.window)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid window size"))?;

        let writer = self.writers.get_mut(&window)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Writer not found"))?;

        let json = serde_json::to_string(metrics)?;
        writeln!(writer, "{}", json)?;

        // Flush every 5 seconds
        if self.last_flush.elapsed() > Duration::from_secs(5) {
            self.flush()?;
            self.last_flush = Instant::now();
        }

        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        for writer in self.writers.values_mut() {
            writer.flush()?;
        }
        Ok(())
    }
}

impl Drop for EnrichedMetricsWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[async_trait]
impl AggregatorWriterBackend for EnrichedMetricsWriter {
    async fn write_metrics(&mut self, metrics: &EnrichedMetrics) -> Result<(), AggregatorWriterError> {
        self.write_metrics(metrics)?;
        Ok(())
    }
    
    async fn flush(&mut self) -> Result<(), AggregatorWriterError> {
        self.flush()?;
        Ok(())
    }
    
    fn backend_type(&self) -> &'static str {
        "JSONL"
    }
}
