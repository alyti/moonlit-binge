use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum DownloaderStatus {
    SegmentProgressReport {
        done: usize,
        total: usize,
        eta: String,
        eta_seconds: usize,
    },
    SegmentFailed {
        segment_id: usize,
        error: String,
    },
    Finished {
        elapsed: std::time::Duration,
    },
}
