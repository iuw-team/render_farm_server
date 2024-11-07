use std::{collections::HashSet, time::SystemTime};

use serde_with::{serde_as, TimestampSeconds};

use crate::{FrameId, WorkerId};

#[serde_as]
#[derive(serde::Serialize, Debug, Clone)]
pub struct WorkerTask {
    pub worker_id: WorkerId,
    #[serde_as(as = "TimestampSeconds<i64>")]
    pub lease_time: SystemTime,
    pub frames: HashSet<FrameId>,
}

impl WorkerTask {
    pub fn new(worker_id: WorkerId, frames: &[FrameId]) -> Self {
        Self {
            worker_id,
            frames: HashSet::from_iter(frames.iter().cloned()),
            lease_time: SystemTime::now(),
        }
    }
}
