use dut::{AppConfig, StartupError, run};

/// musl's allocator serialises allocations across threads, which throttles a
/// multi-threaded Tokio runtime, so static builds such as the container image
/// use mimalloc instead.
#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    dut_telemetry::init()?;
    run(AppConfig::from_env()?).await
}
