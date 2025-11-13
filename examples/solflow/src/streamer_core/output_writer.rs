use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use async_trait::async_trait;
use crate::streamer_core::writer_backend::{WriterBackend, WriterError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEvent {
    pub timestamp: i64,
    pub signature: String,
    pub program_id: String,
    pub program_name: String,
    pub action: String,
    pub mint: String,
    pub sol_amount: f64,
    pub token_amount: f64,
    pub token_decimals: u8,
    pub user_account: Option<String>,
    pub discriminator: String,
}

pub struct JsonlWriter {
    file: BufWriter<File>,
    current_size: u64,
    max_size: u64,
    base_path: PathBuf,
    rotation_count: u32,
    max_rotations: u32,
}

impl JsonlWriter {
    pub fn new(path: impl AsRef<Path>, max_size_mb: u64, max_rotations: u32) -> Result<Self, WriterError> {
        let path = path.as_ref();
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        let current_size = file.metadata()?.len();
        let max_size = max_size_mb * 1024 * 1024;

        Ok(Self {
            file: BufWriter::new(file),
            current_size,
            max_size,
            base_path: path.to_path_buf(),
            rotation_count: 0,
            max_rotations,
        })
    }

    pub fn write_event(&mut self, event: &TradeEvent) -> Result<(), WriterError> {
        let json = serde_json::to_string(event)?;
        writeln!(self.file, "{}", json)?;
        self.file.flush()?;

        self.current_size += (json.len() + 1) as u64;

        if self.current_size >= self.max_size {
            self.rotate()?;
        }

        Ok(())
    }

    fn rotate(&mut self) -> Result<(), WriterError> {
        self.file.flush()?;
        let _ = self.file.get_mut();

        for i in (1..self.max_rotations).rev() {
            let old_path = self.base_path.with_extension(format!("jsonl.{}", i));
            let new_path = self.base_path.with_extension(format!("jsonl.{}", i + 1));
            
            if old_path.exists() {
                if i + 1 > self.max_rotations {
                    std::fs::remove_file(&old_path)?;
                } else {
                    std::fs::rename(&old_path, &new_path)?;
                }
            }
        }

        let rotated_path = self.base_path.with_extension("jsonl.1");
        if self.base_path.exists() {
            std::fs::rename(&self.base_path, &rotated_path)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.base_path)?;

        self.file = BufWriter::new(file);
        self.current_size = 0;
        self.rotation_count += 1;

        log::info!("📄 Rotated output file (rotation #{})", self.rotation_count);

        Ok(())
    }
}

#[async_trait]
impl WriterBackend for JsonlWriter {
    async fn write(&mut self, event: &TradeEvent) -> Result<(), WriterError> {
        self.write_event(event)?;
        Ok(())
    }
    
    async fn flush(&mut self) -> Result<(), WriterError> {
        self.file.flush()?;
        Ok(())
    }
    
    fn backend_type(&self) -> &'static str {
        "JSONL"
    }
}
