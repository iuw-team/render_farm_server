use std::{
    collections::{HashMap, HashSet},
    sync::Arc, time::SystemTime,
};

use crate::{task::WorkerTask, FrameId, WorkerId, LEASE_TIME};

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
        worker_id: WorkerId,
        frame_ids: &[FrameId],
    ) -> WorkerTask {
        self.pending_tasks
            .entry(worker_id.clone())
            .and_modify(|task| {
                for id in frame_ids {
                    let was_inserted = task.frames.insert(*id);
                    assert!(was_inserted);
                }

                task.lease_time += LEASE_TIME;
            })
            .or_insert(WorkerTask::new(worker_id.clone(), frame_ids));

        WorkerTask::new(worker_id.clone(), frame_ids)
    }

    pub fn get_pending_frame_id(&self, worker_id: WorkerId) -> Option<FrameId> {
        let task = self.pending_tasks.get(&worker_id)?;

        if task.frames.len() == 1 {
            task.frames.iter().next().cloned()
        } else {
            None
        }
    }

    //perform clean up old workers
    pub fn clean_up(&mut self) {
        let mut purged_frames = Vec::<FrameId>::new();

        self.pending_tasks.retain(|_id, state| {
            let is_alive = state.lease_time < SystemTime::now();

            if !is_alive {
                purged_frames.extend(state.frames.iter());
            }

            is_alive
        });

        self.frames.extend(purged_frames.iter());
    }

    // true if all frames are successfully generated
    // false if any frame is pending
    pub fn has_frames(&self) -> bool {
        let has_no_pending = self.pending_tasks.values()
            .all(|task| task.frames.is_empty());

        !(self.frames.is_empty() && has_no_pending)
    }
}
