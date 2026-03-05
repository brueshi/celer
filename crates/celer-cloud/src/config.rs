/// Configuration for the cloud compilation service.
pub struct CloudConfig {
    pub host: String,
    pub port: u16,
    /// Maximum source code size in bytes (default: 1 MB).
    pub max_source_bytes: usize,
    /// Compilation timeout in seconds (default: 30).
    pub compile_timeout_secs: u64,
    /// Job time-to-live in seconds (default: 3600 = 1 hour).
    pub job_ttl_secs: u64,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 9000,
            max_source_bytes: 1_048_576,
            compile_timeout_secs: 30,
            job_ttl_secs: 3600,
        }
    }
}
