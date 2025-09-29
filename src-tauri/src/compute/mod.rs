mod timelapse;
mod timeline;
mod workers;

use std::{path::Path, sync::Arc, time::Duration};

use crate::{compute::timelapse::TimelapseEncoder, JobInfo, SetProgressInfo};
use anyhow::Context;
use timeline::Timeline;

pub enum TimelapseType {
    Jpg,
    Mp4,
}
enum DynTimelapseEnc {
    Jpg(timelapse::JpgTimelapseEnc),
    Mp4(timelapse::Mp4TimelapseEnc),
}
impl TimelapseEncoder for DynTimelapseEnc {
    fn encode_frame(&mut self, jpg_data: Vec<u8>) -> anyhow::Result<()> {
        match self {
            Self::Jpg(e) => e.encode_frame(jpg_data),
            Self::Mp4(e) => e.encode_frame(jpg_data),
        }
    }
    fn finish(self) -> anyhow::Result<()> {
        match self {
            Self::Jpg(e) => e.finish(),
            Self::Mp4(e) => e.finish(),
        }
    }
}

pub struct ProcessClipsJob {
    pool: workers::WorkerPool,
    timeline: Arc<timeline::Timeline>,
}
impl ProcessClipsJob {
    pub fn new(threads: usize, info: Arc<JobInfo>, input_path: &str) -> anyhow::Result<Self> {
        let pool = workers::WorkerPool::new(threads);
        let timeline = Timeline::new_from_path(info, &pool, input_path)
            .context("create Timeline from path")?;

        Ok(Self {
            pool,
            timeline: Arc::new(timeline),
        })
    }

    pub fn create_timelapse<P: AsRef<Path>>(
        &self,
        info: Arc<JobInfo>,
        typ: TimelapseType,
        length: Duration,
        fps: u32,
        skip: Option<u32>,
        output_dir: P,
    ) -> anyhow::Result<()> {
        info.set_progress(SetProgressInfo::detail("--- Begin timelapsing ---"));
        let enc = match typ {
            TimelapseType::Jpg => {
                DynTimelapseEnc::Jpg(timelapse::JpgTimelapseEnc::new(output_dir.as_ref()))
            }
            TimelapseType::Mp4 => DynTimelapseEnc::Mp4(
                timelapse::Mp4TimelapseEnc::new(output_dir.as_ref().join("output.mp4"), fps)
                    .context("create mp4 timelapse encoder")?,
            ),
        };
        timelapse::timelapse(
            Arc::clone(&info),
            Arc::clone(&self.timeline),
            &self.pool,
            enc,
            length,
            fps,
            skip,
        )
        .context("create timelapse")?;
        info.set_progress(SetProgressInfo::detail("--- Finished timelapsing ---"));
        Ok(())
    }
}
