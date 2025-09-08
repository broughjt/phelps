use clap::{Parser, Subcommand};
use serde_derive::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Watch,
    Compile
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub root: PathBuf
}
