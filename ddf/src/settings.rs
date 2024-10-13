use anyhow::Result;
use clap::ArgMatches;
use config::{Config, Environment, File};
use directories::ProjectDirs;
use serde::Deserialize;
use std::path::Path;
use tracing::{debug, instrument};

#[derive(Debug, Deserialize)]
pub(crate) struct Settings {
  /// Exclusion list for mounts
  pub(crate) exclude: Option<Vec<Exclusion>>,
  /// Thredsholds for
  pub(crate) threshold: Option<ColorThreshold>,
}

#[derive(Debug, Deserialize)]
pub(crate) enum Exclusion {
  #[serde(rename = "mount_dir_starts_with")]
  MountDirStartsWith(String),
  #[serde(rename = "fstype")]
  FsType(String),
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ColorThreshold {
  pub(crate) medium: Option<f64>,
  pub(crate) high: Option<f64>,
}

const MEDIUM_DEFAULT: f64 = 0.75;
const HIGH_DEFAULT: f64 = 0.90;

impl Default for ColorThreshold {
  fn default() -> Self {
    Self {
      medium: Some(MEDIUM_DEFAULT),
      high: Some(HIGH_DEFAULT),
    }
  }
}

impl Settings {
  pub(crate) fn medium_threshold(&self) -> f64 {
    self
      .threshold
      .clone()
      .unwrap_or_default()
      .medium
      .unwrap_or(MEDIUM_DEFAULT)
  }

  pub(crate) fn high_threshold(&self) -> f64 {
    self
      .threshold
      .clone()
      .unwrap_or_default()
      .high
      .unwrap_or(HIGH_DEFAULT)
  }
}

#[instrument(skip(_matches))]
pub(crate) fn settings(_matches: &ArgMatches) -> Result<Settings> {
  let qualifier: &str = "org";
  let organisation: &str = "djedi";
  let application: &str = "ddf";
  let env_prefix: &str = "DDF";
  let mut settings_builder = Config::builder();
  settings_builder = settings_builder.set_default("uri", "http://localhost:8080")?;

  if let Some(proj_dirs) = ProjectDirs::from(qualifier, organisation, application) {
    let path = Path::new(proj_dirs.config_dir()).join("settings.toml");
    let path = path.to_str().unwrap();
    settings_builder = settings_builder.add_source(File::with_name(path).required(false));
    settings_builder = settings_builder.set_default("configuration_path", path)?;

    debug!("Try to load config file: {}", &path);
  }
  settings_builder = settings_builder.add_source(Environment::with_prefix(env_prefix));
  let config = settings_builder.build()?;
  let settings: Settings = config.clone().try_deserialize()?;

  debug!("{:#?}", settings);

  Ok(settings)
}
