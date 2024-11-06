pub mod env;

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};

use actix_multipart::form::{tempfile::TempFile, MultipartForm};
use actix_web::{
    get,
    http::{header::HeaderName, StatusCode},
    post, put, App, HttpRequest, HttpResponse, HttpServer, Responder,
};

#[derive(MultipartForm)]
struct Frame {
    frame: TempFile,
}

pub type FrameId = u64;
pub type WorkerId = String;

const LEASE_TIME: Duration = Duration::from_secs(1200);
const HEADER_WORKER_ID: HeaderName = HeaderName::from_static("x-worker-id");

pub type StateLock = Arc<RwLock<SharedState>>;

#[derive(Debug, Clone)]
pub struct SharedState {
    pub source_file: Arc<Vec<u8>>,
    pub frames: HashSet<FrameId>,
    pub next_worker_id: u64,
    pub pending_tasks: HashMap<WorkerId, WorkerTask>,
    pub output_directory: String,
}

pub enum WorkerStateError {
    NotFound,
    Retired,
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

#[derive(serde::Serialize, Debug, Clone)]
pub struct WorkerTask {
    pub worker_id: WorkerId,
    pub frame_id: FrameId,
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

#[get("/frames")]
async fn get_frames(req: HttpRequest) -> impl Responder {
    let state_lock = req.app_data::<RwLock<SharedState>>().unwrap();

    let file = {
        let state = state_lock.read().unwrap();

        Arc::clone(&state.source_file)
    };

    HttpResponse::Ok().body(Vec::clone(&file))
}

#[post("/tasks")]
async fn get_next_task(req: HttpRequest) -> impl Responder {
    let state_lock = req.app_data::<StateLock>().unwrap();

    let mut state = state_lock.write().unwrap();

    let Some(frame_id) = state.take_frame_id() else {
        return HttpResponse::new(StatusCode::CONFLICT);
    };

    let worker_id = state.create_worker();

    let task = state.add_task(frame_id, worker_id);

    drop(state);

    HttpResponse::Ok().body(serde_json::to_string(&task).unwrap())
}

#[put("/tasks")]
async fn submit_pending_task(
    req: HttpRequest,
    MultipartForm(form): MultipartForm<Frame>,
) -> impl Responder {
    let Some(worker_id) = req.headers().get(HEADER_WORKER_ID) else {
        return HttpResponse::new(StatusCode::UNAUTHORIZED);
    };

    let worker_id = worker_id.to_str().unwrap();

    let state_lock = req.app_data::<StateLock>().unwrap();

    let mut state = state_lock.write().unwrap();

    let Some(worker_task) = state.pending_tasks.remove(worker_id) else {
        return HttpResponse::new(StatusCode::NOT_FOUND);
    };

    let file_name =
        format!("./{}/{}", state.output_directory, worker_task.frame_id);

    drop(state);

    match form.frame.file.persist(file_name) {
        Ok(_) => {
            let mut state = state_lock.write().unwrap();
            if let Some(frame_id) = state.take_frame_id() {
                let task = state.add_task(frame_id, worker_id.to_string());

                HttpResponse::Ok().body(serde_json::to_string(&task).unwrap())
            } else {
                HttpResponse::new(StatusCode::CONFLICT)
            }
        }
        Err(cause) => {
            log::warn!("Failed to save frame: {cause}");

            HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[post("/workers/alive")]
async fn submit_heartbeat(req: HttpRequest) -> impl Responder {
    let Some(worker_id) = req.headers().get(HEADER_WORKER_ID) else {
        return HttpResponse::new(StatusCode::UNAUTHORIZED);
    };

    let worker_id = worker_id.to_str().unwrap();

    let state_lock = req.app_data::<StateLock>().unwrap();

    let mut state = state_lock.write().unwrap();
    let Some(worker_task) = state.pending_tasks.get_mut(worker_id) else {
        return HttpResponse::new(StatusCode::NOT_FOUND);
    };

    worker_task.lease_time = SystemTime::now() + LEASE_TIME;

    HttpResponse::new(StatusCode::OK)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let output_directory =
        parse_env!("OUT_DIRECTORY", String).unwrap_or("frames".to_string());

    let source_file = {
        let file_path = parse_env!("SRC_FILE", String)
            .unwrap_or("shamyna.blend".to_string());

        std::fs::read(file_path).expect("Failed to load source file")
    };

    let frames_count = parse_env!("FRAMES_COUNT", u64).unwrap_or(128);

    assert!(frames_count > 0);

    let frames = HashSet::from_iter(0..frames_count);

    let state_lock = Arc::new(RwLock::new(SharedState {
        source_file: Arc::new(source_file),
        output_directory,
        frames,
        pending_tasks: HashMap::new(),
        next_worker_id: 1,
    }));

    let state_lock_ref = Arc::clone(&state_lock);
    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;

            let mut state = state_lock_ref.write().unwrap();

            state
                .pending_tasks
                .retain(|_id, state| state.lease_time < SystemTime::now());
        }
    });

    HttpServer::new(move || {
        App::new()
            .service(get_next_task)
            .service(submit_heartbeat)
            .service(submit_pending_task)
            .app_data(Arc::clone(&state_lock))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
