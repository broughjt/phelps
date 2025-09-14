use std::{fs, io, path::PathBuf};

use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use serde_derive::Deserialize;
use thiserror::Error;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Watch,
    Compile,
}

#[derive(Debug, Deserialize)]
pub struct ConfigToml {
    pub project_directory: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub data_directory: PathBuf,
    pub cache_directory: PathBuf,
    pub project_directory: PathBuf,
    pub notes_subdirectory: PathBuf,
    pub build_subdirectory: PathBuf,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("home directory missing, cannot determine project directories")]
    MissingHomeDirectory,
    #[error("couldn't read config.toml file")]
    ConfigRead(io::Error),
    #[error("couldn't parse config.toml file")]
    ConfigParse(toml::de::Error),
    #[error("project directory does not exist")]
    MissingProjectDirectory,
    #[error("notes subdirectory does not exist")]
    MissingNotesSubdirectory,
}

impl Config {
    pub fn try_build() -> Result<Self, ConfigError> {
        let project_directories =
            ProjectDirs::from("", "", "phelps").ok_or(ConfigError::MissingHomeDirectory)?;

        let data_directory = project_directories.data_dir().to_owned();
        let cache_directory = project_directories.data_dir().to_owned();

        let config_path: PathBuf = project_directories.config_dir().join("config.toml");
        let contents = fs::read_to_string(&config_path).map_err(ConfigError::ConfigRead)?;
        let ConfigToml { project_directory } =
            toml::from_str(&contents).map_err(ConfigError::ConfigParse)?;

        let notes_subdirectory = project_directory.join("notes");
        let build_subdirectory = project_directory.join("build");

        if !project_directory.exists() {
            return Err(ConfigError::MissingProjectDirectory);
        }
        if !notes_subdirectory.exists() {
            return Err(ConfigError::MissingNotesSubdirectory);
        }

        Ok(Config {
            data_directory,
            cache_directory,
            project_directory,
            notes_subdirectory,
            build_subdirectory,
        })
    }
}
