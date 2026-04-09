/// S3-Queue configuration with defaults from SPEC §6.2.
#[derive(Debug, Clone)]
pub struct S3QueueConfig {
    pub s3_endpoint: String,
    pub s3_bucket: String,
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,

    /// Maximum messages per batch (default: 1000).
    pub max_batch_size: usize,
    /// Maximum total payload bytes per batch (default: 10 MB).
    pub max_batch_bytes: usize,
    /// Maximum CAS loop attempts before error (default: 10).
    pub max_cas_retries: u32,
    /// Exponential backoff base in ms (default: 50).
    pub cas_backoff_base_ms: u64,
    /// Maximum backoff delay in ms (default: 5000).
    pub cas_backoff_max_ms: u64,
    /// Maximum retries for GET-after-LIST 404 (default: 3).
    pub ceiling_get_max_retries: u32,
    /// Minimum age (seconds) before orphan data eligible for VACUUM (default: 3600).
    pub vacuum_retention_period_s: u64,
}

impl S3QueueConfig {
    pub fn new(endpoint: String, bucket: String) -> Self {
        Self {
            s3_endpoint: endpoint,
            s3_bucket: bucket,
            s3_access_key: None,
            s3_secret_key: None,
            max_batch_size: 1000,
            max_batch_bytes: 10_485_760,
            max_cas_retries: 10,
            cas_backoff_base_ms: 50,
            cas_backoff_max_ms: 5000,
            ceiling_get_max_retries: 3,
            vacuum_retention_period_s: 3600,
        }
    }

    /// Compute backoff delay for a given attempt (0-indexed).
    pub fn backoff_ms(&self, attempt: u32) -> u64 {
        let delay = self.cas_backoff_base_ms.saturating_mul(1u64 << attempt.min(20));
        delay.min(self.cas_backoff_max_ms)
    }
}
