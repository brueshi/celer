use anyhow::{Context, Result};
use clap::Args;

#[derive(Debug, Args)]
pub struct CloudArgs {
    /// Host address to bind
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Port to listen on
    #[arg(short, long, default_value_t = 9000)]
    pub port: u16,

    /// Maximum source code size in bytes
    #[arg(long, default_value_t = 1_048_576)]
    pub max_source_bytes: usize,

    /// Compilation timeout in seconds
    #[arg(long, default_value_t = 30)]
    pub compile_timeout: u64,

    /// Job time-to-live in seconds
    #[arg(long, default_value_t = 3600)]
    pub job_ttl: u64,
}

pub fn execute(args: &CloudArgs) -> Result<()> {
    let config = celer_cloud::CloudConfig {
        host: args.host.clone(),
        port: args.port,
        max_source_bytes: args.max_source_bytes,
        compile_timeout_secs: args.compile_timeout,
        job_ttl_secs: args.job_ttl,
    };

    println!("Starting Celer cloud compilation service");
    println!("  Endpoint: http://{}:{}", config.host, config.port);
    println!("  Max source: {} bytes", config.max_source_bytes);
    println!("  Compile timeout: {}s", config.compile_timeout_secs);
    println!("  Job TTL: {}s", config.job_ttl_secs);
    println!();

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let server = celer_cloud::CloudServer::new(config);
        server
            .run()
            .await
            .map_err(|e| anyhow::anyhow!("cloud server error: {e}"))
    })
}
