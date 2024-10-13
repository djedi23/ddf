use anyhow::Result;

pub fn init_tracing() -> Result<()> {
  #[cfg(any(feature = "console", feature = "forest"))]
  {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::{prelude::*, EnvFilter};
    let registry = tracing_subscriber::registry()
      .with(EnvFilter::from_default_env())
      .with(ErrorLayer::default());
    #[cfg(feature = "console")]
    registry.with(tracing_subscriber::fmt::layer().compact());

    #[cfg(feature = "forest")]
    registry.with(tracing_forest::ForestLayer::default());
  }
  Ok(())
}
