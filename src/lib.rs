pub mod bundle;
pub mod cli;
pub mod commands;
pub mod patch;
pub mod platform;
pub mod upstream;
pub mod ver;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
