// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Options

use super::Error;
use log::warn;
use std::env::var;
use std::path::PathBuf;
pub use wgpu::{BackendBit, PowerPreference};

/// Config mode
///
/// See [`Options::from_env`] documentation.
#[derive(Clone, PartialEq, Hash)]
pub enum ConfigMode {
    /// Read-only mode
    Read,
    /// Use default config and write out
    WriteDefault,
}

/// Shell options
#[derive(Clone, PartialEq, Hash)]
pub struct Options {
    /// Config file path. Default: empty. See `KAS_CONFIG` doc.
    pub config_path: PathBuf,
    /// Config mode. Default: Read.
    pub config_mode: ConfigMode,
    /// Adapter power preference. Default value: low power.
    pub power_preference: PowerPreference,
    /// Adapter backend. Default value: PRIMARY (Vulkan/Metal/DX12).
    pub backends: BackendBit,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            config_path: PathBuf::new(),
            config_mode: ConfigMode::Read,
            power_preference: PowerPreference::LowPower,
            backends: BackendBit::PRIMARY,
        }
    }
}

impl Options {
    /// Construct a new instance, reading from environment variables
    ///
    /// The following environment variables are read, in case-insensitive mode.
    ///
    /// ### Config
    ///
    /// The `KAS_CONFIG` variable, if given, provides a path to the KAS config
    /// file, where configuration can be read and/or written.
    ///
    /// WARNING: file formats are unstable!
    ///
    /// If `KAS_CONFIG` is not set, platform-default configuration is used
    /// without reading or writing. This may change to use a platform-specific
    /// default path in future versions.
    ///
    /// The `KAS_CONFIG_MODE` variable determines the read/write mode:
    ///
    /// -   `Read` (default): read-only
    /// -   `WriteDefault`: generate platform-default configuration, and write
    ///     it to the config path, overwriting any existing config
    ///
    /// Note: in the future, the default will likely change to a read-write mode,
    /// allowing changes to be written out.
    ///
    /// ### Power preference
    ///
    /// The `KAS_POWER_PREFERENCE` variable supports:
    ///
    /// -   `Default`
    /// -   `LowPower`
    /// -   `HighPerformance`
    ///
    /// ### Backend
    ///
    /// The `KAS_BACKENDS` variable supports:
    ///
    /// -   `Vulkan`
    /// -   `GL`
    /// -   `Metal`
    /// -   `DX11`
    /// -   `DX12`
    /// -   `PRIMARY`: any of Vulkan, Metal or DX12
    /// -   `SECONDARY`: any of GL or DX11
    pub fn from_env() -> Self {
        let mut options = Options::default();

        if let Ok(v) = var("KAS_CONFIG") {
            options.config_path = v.into();
        }

        if let Ok(mut v) = var("KAS_CONFIG_MODE") {
            v.make_ascii_uppercase();
            options.config_mode = match v.as_str() {
                "READ" => ConfigMode::Read,
                "WRITEDEFAULT" => ConfigMode::WriteDefault,
                other => {
                    warn!("Unexpected environment value: KAS_CONFIG_MODE={}", other);
                    options.config_mode
                }
            };
        }

        if let Ok(mut v) = var("KAS_POWER_PREFERENCE") {
            v.make_ascii_uppercase();
            options.power_preference = match v.as_str() {
                "DEFAULT" | "LOWPOWER" => PowerPreference::LowPower,
                "HIGHPERFORMANCE" => PowerPreference::HighPerformance,
                other => {
                    warn!(
                        "Unexpected environment value: KAS_POWER_PREFERENCE={}",
                        other
                    );
                    options.power_preference
                }
            }
        }

        if let Ok(mut v) = var("KAS_BACKENDS") {
            v.make_ascii_uppercase();
            options.backends = match v.as_str() {
                "VULKAN" => BackendBit::VULKAN,
                "GL" => BackendBit::GL,
                "METAL" => BackendBit::METAL,
                "DX11" => BackendBit::DX11,
                "DX12" => BackendBit::DX12,
                "PRIMARY" => BackendBit::PRIMARY,
                "SECONDARY" => BackendBit::SECONDARY,
                other => {
                    warn!("Unexpected environment value: KAS_BACKENDS={}", other);
                    options.backends
                }
            }
        }

        options
    }

    pub(crate) fn adapter_options(&self) -> wgpu::RequestAdapterOptions {
        wgpu::RequestAdapterOptions {
            power_preference: self.power_preference,
            compatible_surface: None,
        }
    }

    pub(crate) fn backend(&self) -> BackendBit {
        self.backends
    }

    /// Load KAS config
    pub fn config(&self) -> Result<kas::event::Config, Error> {
        if !self.config_path.as_os_str().is_empty() {
            match self.config_mode {
                ConfigMode::Read => Ok(kas::event::Config::from_path(
                    &self.config_path,
                    Default::default(),
                )?),
                ConfigMode::WriteDefault => {
                    let config: kas::event::Config = Default::default();
                    config.write_path(&self.config_path, Default::default())?;
                    Ok(config)
                }
            }
        } else {
            Ok(Default::default())
        }
    }
}
