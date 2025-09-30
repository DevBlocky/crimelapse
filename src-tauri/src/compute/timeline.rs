use crate::{compute::workers::WorkerPool, SetProgressInfo};

use super::JobInfo;
use anyhow::Context;
use std::{
    error::Error,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub struct TimelineClip {
    /// start offset of the clip within the timeline
    pub creation_time: chrono::DateTime<chrono::Utc>,
    /// runtime of the clip
    pub length: Duration,
    /// the path to the clip
    pub path: PathBuf,
}
impl TimelineClip {
    fn process(job: &JobInfo, path: PathBuf) -> anyhow::Result<Self> {
        job.cancel_result()?;

        let info = crate::ffmpeg::probe(&path).context("probe info")?;
        let creation_time =
            Self::parse_timestamp_from_path(&path).context("parse timestamp from path")?;

        job.set_progress(SetProgressInfo::detail(format!(
            "processed TimelineClip {}",
            path.to_string_lossy()
        )));
        Ok(Self {
            creation_time,
            length: info.duration,
            path,
        })
    }

    fn parse_timestamp_from_path(path: &Path) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
        use chrono::{NaiveDateTime, TimeZone};

        let filename = path
            .file_name()
            .map(OsStr::to_string_lossy)
            .ok_or(anyhow::anyhow!("get filename from path"))?;
        let date_str = &filename[..16]; // the first 16 characters includes the date: YYYY_MMDD_HHmmss
        let ndt = NaiveDateTime::parse_from_str(date_str, "%Y_%m%d_%H%M%S")?;
        chrono_tz::America::New_York
            .from_local_datetime(&ndt)
            .single()
            .map(|dt| dt.to_utc())
            .ok_or(anyhow::anyhow!("from_local_datetime not single"))
    }
}

pub struct Timeline {
    clips: Vec<(Duration, TimelineClip)>,
    duration: Duration,
}
impl Timeline {
    pub fn new_from_path(
        info: Arc<JobInfo>,
        pool: &WorkerPool,
        input_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let glob_pattern = input_path.as_ref().join("**").join("*.mp4");
        let paths = glob::glob_with(
            &glob_pattern.to_string_lossy(),
            glob::MatchOptions {
                case_sensitive: false,
                ..Default::default()
            },
        )?;
        Self::new(info, pool, paths)
    }
    fn new<E: Error + Send + Sync + 'static>(
        info: Arc<JobInfo>,
        pool: &WorkerPool,
        paths: impl Iterator<Item = Result<PathBuf, E>>,
    ) -> anyhow::Result<Self> {
        info.set_progress(crate::SetProgressInfo {
            progress: Some(0),
            total: Some(0),
            detail: Some("--- Starting to timeline clips... ---".to_string()),
            ..Default::default()
        });

        // create and run jobs to process the TimelineClip for each path specified
        let clips_rx = pool.run_channel(paths.map(|path| {
            let info_clone = info.clone();
            move || {
                let path = path?;
                TimelineClip::process(&info_clone, path.clone())
                    .with_context(|| format!("process TimelineClip {:?}", path))
            }
        }));

        // collect all of the TimelineClips into a vector and sort by creation_time
        let mut timeline_clips = Vec::new();
        for clip in clips_rx {
            timeline_clips.push(clip?);
        }
        timeline_clips.sort_unstable_by_key(|x| x.creation_time);

        // finally, create a vec with a duration before the clip
        let mut duration = Duration::ZERO;
        let mut clips = Vec::new();
        for clip in timeline_clips {
            let len = clip.length;
            clips.push((duration, clip));
            duration += len;
        }

        info.set_progress(SetProgressInfo::detail(format!(
            "Total combined length of all clips is {:.02}h",
            duration.as_secs_f64() / 60.0 / 60.0
        )));
        info.set_progress(SetProgressInfo::detail("--- Finished clips timeline ---"));
        Ok(Self { clips, duration })
    }

    pub fn get_at(&self, timestamp: Duration) -> (Duration, &TimelineClip) {
        let idx = match self
            .clips
            .binary_search_by_key(&timestamp, |(clip_ts, _)| *clip_ts)
        {
            Ok(i) => i,
            Err(i) => i - 1, // since this is where it should be "inserted", we need the previous one
        };
        (self.clips[idx].0, &self.clips[idx].1)
    }
    pub fn len(&self) -> Duration {
        self.duration
    }
}
