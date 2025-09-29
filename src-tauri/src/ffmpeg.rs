use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::OnceLock,
    time::Duration,
};

use anyhow::{anyhow, Context};
use tauri::{path::BaseDirectory, AppHandle, Manager};

// Relative locations of bundled ffmpeg binaries.
#[cfg(target_os = "macos")]
const FFMPEG_RELATIVE_PATH: &str = "resources/bin/mac/ffmpeg";
#[cfg(target_os = "macos")]
const FFPROBE_RELATIVE_PATH: &str = "resources/bin/mac/ffprobe";

#[cfg(not(target_os = "macos"))]
compile_error!("Bundled ffmpeg binaries are only configured for macOS.");

#[derive(Debug)]
struct Binaries {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

static BINARIES: OnceLock<Binaries> = OnceLock::new();

pub fn set_paths(app: &AppHandle) -> anyhow::Result<()> {
    BINARIES
        .set(Binaries {
            ffmpeg: resolve_resource(app, FFMPEG_RELATIVE_PATH)?,
            ffprobe: resolve_resource(app, FFPROBE_RELATIVE_PATH)?,
        })
        .map_err(|_| anyhow::anyhow!("could not set ffmpeg::BINARIES"))?;
    Ok(())
}
fn binaries() -> &'static Binaries {
    BINARIES.get().expect("binaries set by lib.rs")
}

fn resolve_resource(app: &AppHandle, relative: &str) -> anyhow::Result<PathBuf> {
    match app.path().resolve(relative, BaseDirectory::Resource) {
        Ok(path) => Ok(path),
        Err(err) => {
            let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
            if fallback.exists() {
                Ok(fallback)
            } else {
                Err(anyhow!("failed to resolve resource {relative}: {err}"))
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct ProbeDurOutput {
    format: FFProbeFormat,
}
#[derive(Debug, serde::Deserialize)]
struct FFProbeFormat {
    // ffprobe, WHY THE FUCK IS THIS A STRING????
    duration: String,
}
#[derive(Debug)]
pub struct ProbeInfo {
    pub duration: Duration,
}
pub fn probe(path: &Path) -> anyhow::Result<ProbeInfo> {
    let bins = binaries();

    #[rustfmt::skip]
    let result = Command::new(&bins.ffprobe)
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-probesize", "32k",
            "-show_entries", "format",
            "-of", "json",
        ])
        .arg(path)
        .output()
        .context("execute probe")?;

    // if there was an error, bail
    if !result.status.success() {
        anyhow::bail!(
            "ffprobe for duration failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )
    }

    // parse the json output from ffprobe for the duration
    let output =
        serde_json::from_slice::<ProbeDurOutput>(&result.stdout).context("parse ProbeDurOutput")?;

    let dur_secs = output
        .format
        .duration
        .parse::<f64>()
        .context("parse ProbeDurOutput.format.duration")?;

    Ok(ProbeInfo {
        duration: Duration::from_secs_f64(dur_secs),
    })
}

pub fn extract_frame(input: &Path, at: Duration) -> anyhow::Result<Vec<u8>> {
    let bins = binaries();

    #[rustfmt::skip]
    let result = Command::new(&bins.ffmpeg)
        .arg("-v").arg("error")
        .arg("-ss").arg(&at.as_secs_f64().to_string())
        .arg("-i").arg(input)
        .arg("-frames:v").arg("1")
        .arg("-f").arg("image2")
        .arg("-vcodec").arg("mjpeg")
        .arg("-q:v").arg("2")
        .arg("-")
        .output()
        .context("execute ffmpeg to extract frame")?;

    if !result.status.success() {
        anyhow::bail!(
            "ffmpeg frame extraction failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    if result.stdout.is_empty() {
        extract_last_frame(input).context("extract_frame failed -> using extract_last_frame")
    } else {
        Ok(result.stdout)
    }
}
fn extract_last_frame(input: &Path) -> anyhow::Result<Vec<u8>> {
    let bins = binaries();

    #[rustfmt::skip]
    let result = Command::new(&bins.ffmpeg)
        .arg("-v").arg("error")
        .arg("-sseof").arg("-3")
        .arg("-i").arg(input)
        .arg("-update").arg("1")
        .arg("-f").arg("image2")
        .arg("-vcodec").arg("mjpeg")
        .arg("-q:v").arg("2")
        .arg("-")
        .output()
        .context("execute ffmpeg to extract frame")?;

    if !result.status.success() {
        anyhow::bail!(
            "ffmpeg frame extraction failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    if result.stdout.is_empty() {
        anyhow::bail!("ffmpeg did not produce frame data");
    }

    Ok(result.stdout)
}

pub struct Mp4FrameEncoder {
    child: Child,
}
impl Mp4FrameEncoder {
    pub fn new(output: &Path, fps: u32) -> anyhow::Result<Self> {
        let bins = binaries();

        #[rustfmt::skip]
        let child = Command::new(&bins.ffmpeg)
            .arg("-y")
            .arg("-v").arg("error")
            .arg("-f").arg("image2pipe")
            .arg("-vcodec").arg("mjpeg")
            .arg("-r").arg(fps.to_string())
            .arg("-i").arg("-")
            .arg("-c:v").arg("libx264")
            .arg("-pix_fmt").arg("yuv420p")
            .arg("-movflags").arg("+faststart")
            .arg(output)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn ffmpeg mp4 encoder")?;

        Ok(Self { child })
    }

    pub fn encode_frame(&mut self, jpeg: &[u8]) -> anyhow::Result<()> {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("ffmpeg stdin already closed"))?;
        stdin
            .write_all(jpeg)
            .context("write frame to ffmpeg stdin")?;
        stdin.flush().context("flush ffmpeg stdin after frame")?;
        Ok(())
    }

    pub fn finish(&mut self) -> anyhow::Result<()> {
        if let Some(mut stdin) = self.child.stdin.take() {
            stdin.flush().context("flush ffmpeg stdin before finish")?;
        }

        let mut stderr_handle = self.child.stderr.take();
        let status = self
            .child
            .wait()
            .context("wait for ffmpeg encoder to finish")?;

        let mut stderr_buf = Vec::new();
        if let Some(mut stderr) = stderr_handle.take() {
            stderr
                .read_to_end(&mut stderr_buf)
                .context("read ffmpeg stderr")?;
        }

        if !status.success() {
            anyhow::bail!(
                "ffmpeg mp4 encoder failed: {}",
                String::from_utf8_lossy(&stderr_buf)
            );
        }

        Ok(())
    }
}
