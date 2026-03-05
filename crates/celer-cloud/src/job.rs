use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Compiling,
    Complete,
    Failed,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub module_name: String,
    pub status: JobStatus,
    pub created_at: Instant,
    /// Compilation time if completed.
    pub compile_time_ms: Option<u64>,
    /// Path to compiled artifact (.so/.dylib).
    pub artifact_path: Option<PathBuf>,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Thread-safe job store using DashMap for lock-free concurrent access.
pub struct JobStore {
    jobs: Arc<DashMap<String, Job>>,
    ttl: Duration,
}

impl JobStore {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            jobs: Arc::new(DashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    pub fn insert(&self, job: Job) {
        self.jobs.insert(job.id.clone(), job);
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.jobs.get(id).map(|r| r.clone())
    }

    pub fn update_status(&self, id: &str, status: JobStatus) {
        if let Some(mut job) = self.jobs.get_mut(id) {
            job.status = status;
        }
    }

    pub fn mark_complete(&self, id: &str, artifact_path: PathBuf, compile_time_ms: u64) {
        if let Some(mut job) = self.jobs.get_mut(id) {
            job.status = JobStatus::Complete;
            job.artifact_path = Some(artifact_path);
            job.compile_time_ms = Some(compile_time_ms);
        }
    }

    pub fn mark_failed(&self, id: &str, error: String) {
        if let Some(mut job) = self.jobs.get_mut(id) {
            job.status = JobStatus::Failed;
            job.error = Some(error);
        }
    }

    /// Remove expired jobs. Returns number of removed jobs.
    pub fn cleanup_expired(&self) -> usize {
        let now = Instant::now();
        let expired: Vec<String> = self
            .jobs
            .iter()
            .filter(|entry| now.duration_since(entry.value().created_at) > self.ttl)
            .map(|entry| entry.key().clone())
            .collect();
        let count = expired.len();
        for id in expired {
            if let Some((_, job)) = self.jobs.remove(&id) {
                if let Some(path) = &job.artifact_path {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        count
    }
}
