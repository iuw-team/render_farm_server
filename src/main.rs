pub mod env;
pub mod state;
pub mod task;

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};

use actix_multipart::form::{tempfile::TempFile, MultipartForm};
use actix_web::{
    get,
    http::{
        header::{ContentDisposition, ContentType, HeaderName},
        StatusCode,
    },
    post, put,
    web::Query,
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use state::SharedState;

#[derive(MultipartForm)]
struct Frame {
    frame: TempFile,
}

pub type WorkerId = String;

const LEASE_TIME: Duration = Duration::from_secs(1200);
const HEADER_WORKER_ID: HeaderName = HeaderName::from_static("x-worker-id");
const FRAMES_FILE_NAME: &str = "shamyna.blend";

pub type StateLock = Arc<RwLock<SharedState>>;

#[derive(serde::Deserialize)]
pub struct TaskQuery {
    pub count: Option<u32>,
}

#[derive(serde::Deserialize)]
pub struct FrameQuery {
    pub frame_id: Option<u64>,
}

#[get("/frames")]
async fn get_frames(req: HttpRequest) -> impl Responder {
    let state_lock = req.app_data::<StateLock>().unwrap();

    let file = {
        let state = state_lock.read().unwrap();

        Arc::clone(&state.source_file)
    };

    HttpResponse::Ok()
        .content_type(ContentType::octet_stream())
        .insert_header(ContentDisposition::attachment(FRAMES_FILE_NAME))
        .body(Vec::clone(&file))
}

#[post("/tasks")]
async fn get_next_tasks(req: HttpRequest, query: Query<TaskQuery>) -> impl Responder {
    let state_lock = req.app_data::<StateLock>().unwrap();

    let mut state = state_lock.write().unwrap();

    let task_count = query.count.unwrap_or(1);
    let frame_ids = (0..task_count)
        .flat_map(|_| state.take_frame_id())
        .collect::<Vec<u64>>();

    if frame_ids.is_empty() {
        //no task can be generated because all are already taken
        return HttpResponse::new(StatusCode::CONFLICT);
    }

    let worker_id = state.create_worker();

    let task = state.add_task(worker_id, &frame_ids);

    drop(state);

    HttpResponse::Ok().body(serde_json::to_string(&task).unwrap())
}

#[put("/tasks")]
async fn submit_pending_task(
    req: HttpRequest,
    MultipartForm(form): MultipartForm<Frame>,
    query: Query<FrameQuery>,
) -> impl Responder {
    let Some(worker_id) = req.headers().get(HEADER_WORKER_ID) else {
        return HttpResponse::new(StatusCode::UNAUTHORIZED);
    };

    let worker_id = worker_id.to_str().unwrap();

    let state_lock = req.app_data::<StateLock>().unwrap();

    let mut state = state_lock.write().unwrap();

    let Some(frame_id) = query
        .frame_id
        .or(state.get_pending_frame_id(worker_id.to_string()))
    else {
        return HttpResponse::new(StatusCode::BAD_REQUEST);
    };

    let Some(worker_task) = state.pending_tasks.get_mut(worker_id) else {
        return HttpResponse::new(StatusCode::NOT_FOUND);
    };

    if !worker_task.frames.remove(&frame_id) {
        return HttpResponse::new(StatusCode::NOT_FOUND);
    }

    let file_name = format!("{}/{}.png", state.output_directory, frame_id);

    drop(state);

    match form.frame.file.persist(file_name) {
        Ok(_) => {
            let mut state = state_lock.write().unwrap();

            if let Some(frame_id) = state.take_frame_id() {
                let task = state.add_task(worker_id.to_string(), &[frame_id]);

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

    let ok = state.update_heart_beat(worker_id);

    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    HttpResponse::new(status)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::builder().init();

    let output_directory = parse_env!("OUT_DIRECTORY", String).unwrap_or("/tmp/frames".to_string());

    std::fs::remove_dir_all(&output_directory).expect("Failed to clear output directory");

    std::fs::create_dir_all(&output_directory).expect("Failed to create output directory");

    let script = parse_env!("COMMAND", String).unwrap_or("./run.sh".to_string());

    let source_file = {
        let file_path = parse_env!("SOURCE_FILE", String).unwrap_or(FRAMES_FILE_NAME.to_string());

        std::fs::read(file_path).expect("Failed to load source file")
    };

    let frames_count = parse_env!("FRAMES_COUNT", u64).unwrap_or(1);

    assert!(frames_count > 0);

    let frames = HashSet::from_iter(0..frames_count);

    let state_lock = Arc::new(RwLock::new(SharedState {
        source_file: Arc::new(source_file),
        output_directory: output_directory.clone(),
        frames,
        pending_tasks: HashMap::new(),
        next_worker_id: 1,
    }));

    let state_lock_ref = Arc::clone(&state_lock);
    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;

            let mut state = state_lock_ref.write().unwrap();

            state.clean_up();

            let has_frames = state.has_frames();

            drop(state);

            if !has_frames {
                tokio::task::spawn_blocking(move || {
                    let mut child = std::process::Command::new(script)
                        .arg(output_directory)
                        .spawn()
                        .expect("Failed to run build script");

                    let exit_status = child.wait();

                    match exit_status {
                        Ok(code) => {
                            log::info!("Build script exit with code: {code}");
                        }
                        Err(cause) => {
                            log::warn!("Build script failed: {cause}");
                        }
                    }
                });

                log::info!("Master node is dying...");
                break;
            }
        }
    });

    HttpServer::new(move || {
        App::new()
            .service(get_frames)
            .service(get_next_tasks)
            .service(submit_pending_task)
            .service(submit_heartbeat)
            .app_data(Arc::clone(&state_lock))
    })
        .bind(("0.0.0.0", 8080))?
        .run()
        .await
}
