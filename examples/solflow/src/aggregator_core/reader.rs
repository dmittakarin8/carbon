//! Asynchronous JSONL tail reader with file rotation detection

use std::io::SeekFrom;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::time::sleep;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

pub struct TailReader {
    path: PathBuf,
    file: Option<BufReader<File>>,
    inode: Option<u64>,
    poll_interval: Duration,
}

impl TailReader {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            file: None,
            inode: None,
            poll_interval: Duration::from_millis(100),
        }
    }

    /// Start tailing the file (seeks to end)
    pub async fn start(&mut self) -> std::io::Result<()> {
        let file = File::open(&self.path).await?;
        let metadata = file.metadata().await?;
        
        #[cfg(unix)]
        {
            self.inode = Some(metadata.ino());
        }
        
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::End(0)).await?;
        self.file = Some(reader);
        
        log::info!("📖 Started tailing: {}", self.path.display());
        Ok(())
    }

    /// Read the next line, waiting if necessary
    pub async fn read_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            // Check for file rotation
            if self.detect_rotation().await? {
                log::info!("🔄 File rotation detected, reopening: {}", self.path.display());
                self.start().await?;
            }

            if let Some(ref mut reader) = self.file {
                let mut line = String::new();
                match reader.read_line(&mut line).await? {
                    0 => {
                        // No new data, sleep and retry
                        sleep(self.poll_interval).await;
                        continue;
                    }
                    _ => {
                        if !line.trim().is_empty() {
                            return Ok(Some(line.trim().to_string()));
                        }
                        // Empty line, continue reading
                        continue;
                    }
                }
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "File not opened",
                ));
            }
        }
    }

    /// Detect if the file has been rotated (inode changed)
    async fn detect_rotation(&self) -> std::io::Result<bool> {
        #[cfg(unix)]
        {
            let metadata = tokio::fs::metadata(&self.path).await?;
            let current_inode = metadata.ino();
            Ok(self.inode.map_or(false, |old| old != current_inode))
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, check file size decrease as heuristic
            if let Some(ref file) = self.file {
                let current_pos = file.get_ref().stream_position().await?;
                let metadata = tokio::fs::metadata(&self.path).await?;
                Ok(metadata.len() < current_pos)
            } else {
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_tail_reader_basic() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.jsonl");

        // Create and write initial content
        let mut file = tokio::fs::File::create(&file_path).await.unwrap();
        file.write_all(b"line1\nline2\n").await.unwrap();
        file.flush().await.unwrap();
        drop(file);

        // Create reader and start (should seek to end)
        let mut reader = TailReader::new(file_path.clone());
        reader.start().await.unwrap();

        // Append new line
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .await
            .unwrap();
        file.write_all(b"line3\n").await.unwrap();
        file.flush().await.unwrap();
        drop(file);

        // Should only read the new line (line3)
        let line = tokio::time::timeout(Duration::from_secs(2), reader.read_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(line, "line3");
    }
}
