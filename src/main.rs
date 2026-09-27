use dut::{AppConfig, StartupError, run, telemetry};

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    telemetry::init()?;
    run(AppConfig::from_env()?).await
}
