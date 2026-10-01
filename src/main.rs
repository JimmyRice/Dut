use dut::{CommandLine, StartupError, run};

/// musl's allocator serialises allocations across threads, which throttles a
/// multi-threaded Tokio runtime, so static builds such as the container image
/// use mimalloc instead.
#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    let command_line = CommandLine::read();
    dut_telemetry::init(&command_line.log_config())?;
    run(command_line.app_config()).await
}
