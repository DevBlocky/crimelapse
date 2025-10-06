use std::path::Path;

use crate::{JobInfo, SetProgressInfo};

use super::timeline::Timeline;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TimelineExportEntry {
    file_path: String,
    timestamp: String,
    duration: f64,
    location: Option<TimelineExportEntryLocation>,
}
#[derive(Debug, serde::Serialize)]
struct TimelineExportEntryLocation {
    lat: f64,
    lng: f64,
}

pub fn export_timeline(
    info: &JobInfo,
    timeline: &Timeline,
    locs: Option<&[super::glyph::LatLng]>,
    output_dir: &Path,
) -> anyhow::Result<()> {
    let entries = timeline
        .iter()
        .enumerate()
        .map(|(i, clip)| TimelineExportEntry {
            file_path: clip.path.to_string_lossy().into(),
            timestamp: clip.creation_time.to_rfc3339(),
            duration: clip.length.as_secs_f64(),
            location: locs.map(|locs| TimelineExportEntryLocation {
                lat: locs[i].lat,
                lng: locs[i].lng,
            }),
        })
        .collect::<Vec<_>>();
    let output_path = output_dir.join("output.json");
    std::fs::write(&output_path, serde_json::to_string_pretty(&entries)?)?;
    info.set_progress(SetProgressInfo::detail(format!(
        "exported data to file {:?}",
        output_path
    )));
    Ok(())
}
