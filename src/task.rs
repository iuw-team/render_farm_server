use std::time::SystemTime;

use serde_with::{serde_as, TimestampSeconds};

use crate::{FrameId, WorkerId};

#[serde_as]
#[derive(serde::Serialize, Debug, Clone)]
pub struct WorkerTask {
    pub worker_id: WorkerId,
    pub frame_id: FrameId,
    #[serde_as(as = "TimestampSeconds<i64>")]
    pub lease_time: SystemTime,
}

impl WorkerTask {
    pub fn new(worker_id: WorkerId, frame_id: FrameId) -> Self {
        Self {
            worker_id,
            frame_id,
            lease_time: SystemTime::now(),
        }
    }
}
