mod compute;
mod ffmpeg;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc, Mutex,
    },
    time::Duration,
};

use tauri::{path::BaseDirectory, AppHandle, Emitter, Manager, State};

// job info and state //

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SetProgressInfo {
    progress: Option<usize>,
    progress_inc: Option<usize>,
    total: Option<usize>,
    detail: Option<String>,
}
impl SetProgressInfo {
    fn detail<S: Into<String>>(s: S) -> Self {
        Self {
            detail: Some(s.into()),
            ..Default::default()
        }
    }
}
struct JobInfo {
    id: usize,
    is_cancelled: AtomicBool,
    app: AppHandle,
}
impl JobInfo {
    pub(crate) fn set_progress(&self, info: SetProgressInfo) {
        self.app
            .emit(&format!("progress:{}", self.id), info)
            .expect("emit progress");
    }
    pub fn cancelled(&self) -> bool {
        self.is_cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn cancel_result(&self) -> anyhow::Result<()> {
        if self.cancelled() {
            anyhow::bail!("job is cancelled")
        }
        Ok(())
    }
    pub fn resolve_resource<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        self.app
            .path()
            .resolve(path, BaseDirectory::Resource)
            .expect("resolve resource path")
    }
}
struct Jobs {
    id_inc: AtomicUsize,
    active: Mutex<HashMap<usize, Arc<JobInfo>>>,
}

// job options //

#[derive(Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TimelapseType {
    None,
    Jpg,
    Mp4,
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelapseOptions {
    typ: TimelapseType,
    length: u64,
    fps: u32,
    skip: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
struct ExportOptions {
    enabled: bool,
    location: bool,
}

// job commands //

#[tauri::command]
fn start_job(
    app: AppHandle,
    jobs: State<Jobs>,
    threads: usize,
    input_path: String,
    output_path: String,
    timelapse: TimelapseOptions,
    export: ExportOptions,
) -> usize {
    // create the JobInfo struct for this job
    let id = jobs
        .id_inc
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let info = Arc::new(JobInfo {
        id,
        is_cancelled: AtomicBool::new(false),
        app,
    });
    // add the JobInfo struct to the list of currently active jobs
    {
        let mut job_map = jobs.active.lock().unwrap();
        job_map.insert(info.id, info.clone());
    }

    let info_clone = info.clone();
    let run_job = move || -> anyhow::Result<()> {
        let job = compute::ProcessClipsJob::new(threads, Arc::clone(&info_clone), &input_path)?;
        std::fs::create_dir_all(&output_path)?; // create output directory
        if timelapse.typ != TimelapseType::None {
            let typ = match timelapse.typ {
                TimelapseType::Jpg => compute::TimelapseType::Jpg,
                TimelapseType::Mp4 => compute::TimelapseType::Mp4,
                _ => unreachable!(),
            };
            let length = Duration::from_secs(timelapse.length);
            job.create_timelapse(
                Arc::clone(&info_clone),
                typ,
                length,
                timelapse.fps,
                timelapse.skip,
                &output_path,
            )?;
        }
        if export.enabled {
            job.export_data(info_clone, export.location, &output_path)?;
        }
        Ok(())
    };

    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = run_job() {
            let panic_msg = format!("----- PANIC -----\n{:?}\n", e);
            info.set_progress(SetProgressInfo::detail(panic_msg.clone()));
            eprintln!("{}", panic_msg);
        }
        info.is_cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    });
    id
}

#[tauri::command]
fn cancel_job(job_id: usize, jobs: State<Jobs>) -> bool {
    let mut job_map = jobs.active.lock().unwrap();
    let info = job_map.remove(&job_id);
    if let Some(ji) = &info {
        ji.is_cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    info.is_some()
}

// other commands //

#[tauri::command]
fn get_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|x| x.get())
        .unwrap_or(1)
}

#[tauri::command]
fn read_file(filepath: &Path) -> String {
    std::fs::read_to_string(filepath).expect("read file from filepath")
}

// init //

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let jobs_state = Jobs {
        id_inc: AtomicUsize::new(1),
        active: Mutex::new(HashMap::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            ffmpeg::set_paths(app.handle())?;
            Ok(())
        })
        .manage(jobs_state)
        .invoke_handler(tauri::generate_handler![
            start_job,
            cancel_job,
            get_parallelism,
            read_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
