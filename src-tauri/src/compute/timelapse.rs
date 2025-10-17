use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;

use crate::{
    compute::{timeline::Timeline, workers::WorkerPool},
    ffmpeg, JobInfo,
};

pub trait TimelapseEncoder: Sized {
    fn encode_frame(&mut self, jpg_data: Vec<u8>) -> anyhow::Result<()>;
    fn finish(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct JpgTimelapseEnc {
    output_dir: PathBuf,
    frame_n: usize,
}
impl JpgTimelapseEnc {
    pub fn new<P: Into<PathBuf>>(output_dir: P) -> Self {
        Self {
            frame_n: 0,
            output_dir: output_dir.into(),
        }
    }
}
impl TimelapseEncoder for JpgTimelapseEnc {
    fn encode_frame(&mut self, jpg_data: Vec<u8>) -> anyhow::Result<()> {
        self.frame_n += 1;
        std::fs::write(
            self.output_dir.join(&format!("{}.jpg", self.frame_n)),
            jpg_data,
        )?;
        Ok(())
    }
}

pub struct Mp4TimelapseEnc {
    enc: ffmpeg::Mp4FrameEncoder,
}
impl Mp4TimelapseEnc {
    pub fn new<P: AsRef<Path>>(output: P, fps: u32) -> anyhow::Result<Self> {
        Ok(Self {
            enc: ffmpeg::Mp4FrameEncoder::new(output.as_ref(), fps)?,
        })
    }
}
impl TimelapseEncoder for Mp4TimelapseEnc {
    fn encode_frame(&mut self, jpg_data: Vec<u8>) -> anyhow::Result<()> {
        self.enc.encode_frame(&jpg_data)
    }
    fn finish(mut self) -> anyhow::Result<()> {
        self.enc.finish()
    }
}

pub fn timelapse<E: TimelapseEncoder>(
    info: Arc<JobInfo>,
    timeline: Arc<Timeline>,
    pool: &WorkerPool,
    mut enc: E,
    len: Duration,
    fps: u32,
    skip: Option<u32>,
) -> anyhow::Result<()> {
    let num_frames = (len.as_secs_f64() * fps as f64) as u32;
    let timestamps =
        (skip.unwrap_or(0)..=num_frames).map(|frame_n| frame_n * (timeline.len() / num_frames));
    let num_frames = num_frames - skip.unwrap_or(0);

    info.set_progress(crate::SetProgressInfo {
        progress: Some(0),
        total: Some(num_frames as usize),
        ..Default::default()
    });

    let jobs = pool.run_ordered_channel(timestamps.map(|ts| {
        let info = Arc::clone(&info);
        let timeline = Arc::clone(&timeline);
        move || {
            info.cancel_result()?;
            let (clip_ts, clip) = timeline.get_at(ts);
            let ts_in_clip = ts - clip_ts;
            ffmpeg::extract_frame(&clip.path, ts_in_clip).with_context(|| {
                format!(
                    "extract frame from {} @ {:.02}s",
                    clip.path.to_string_lossy(),
                    ts_in_clip.as_secs_f64()
                )
            })
        }
    }));

    for (i, job) in jobs.into_iter().enumerate() {
        let detail = match job.with_context(|| format!("extract frame {}", i)) {
            Ok(jpg_data) => {
                enc.encode_frame(jpg_data)
                    .with_context(|| format!("encode frame {}", i))?;
                format!("encoded frame {}/{}", i, num_frames)
            }
            Err(e) => format!("WARN: could not extract frame {i}/{num_frames}\n{e}\n\n"),
        };
        info.set_progress(crate::SetProgressInfo {
            progress_inc: Some(1),
            detail: Some(detail),
            ..Default::default()
        });
    }
    enc.finish().context("finish encoding")?;
    Ok(())
}
