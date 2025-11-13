pub mod balance_extractor;
pub mod config;
pub mod error_handler;
pub mod grpc_client;
pub mod output_writer;
pub mod trade_detector;
pub mod writer_backend;
pub mod sqlite_writer;

mod lib;

pub use config::{RuntimeConfig, StreamerConfig};
pub use lib::run;
pub use output_writer::TradeEvent;
