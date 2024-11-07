use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{task::WorkerTask, FrameId, WorkerId};

#[derive(Debug, Clone)]
pub struct SharedState {
    pub source_file: Arc<Vec<u8>>,
    pub frames: HashSet<FrameId>,
    pub next_worker_id: u64,
    pub pending_tasks: HashMap<WorkerId, WorkerTask>,
    pub output_directory: String,
}

impl SharedState {
    pub fn take_frame_id(&mut self) -> Option<FrameId> {
        let frame_id = self.frames.iter().next().cloned()?;
        let _ = self.frames.remove(&frame_id);
        Some(frame_id)
    }

    pub fn create_worker(&mut self) -> WorkerId {
        let worker_id = self.next_worker_id.to_string();
        self.next_worker_id += 1;
        worker_id
    }

    pub fn add_task(
        &mut self,
        frame_id: FrameId,
        worker_id: WorkerId,
    ) -> WorkerTask {
        let worker_task = WorkerTask::new(worker_id.clone(), frame_id);

        self.pending_tasks.insert(worker_id, worker_task.clone());

        worker_task
    }
}
