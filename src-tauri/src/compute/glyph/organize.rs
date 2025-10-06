use std::{path::Path, time::Duration};

use crate::{
    compute::{glyph::GlyphConfig, timeline::Timeline},
    ffmpeg, JobInfo, SetProgressInfo,
};

const GLYPH_MASK_SIMILARITY_THRESHOLD: f64 = 0.85;

pub fn organize_glyphs(
    info: &JobInfo,
    timeline: &Timeline,
    gcfg: &GlyphConfig,
    output_dir: &Path,
) -> anyhow::Result<()> {
    info.set_progress(SetProgressInfo::detail("[dbg] begin recognizing glyphs"));

    let mut n_glyphs = 0;
    let mut unique_glyphs = Vec::new();
    for clip in timeline.iter() {
        info.cancel_result()?;

        let jpg_data = ffmpeg::extract_frame(&clip.path, Duration::ZERO)?;
        let rgb = image::load_from_memory(&jpg_data)?.to_rgb8();
        std::mem::drop(jpg_data);

        for row in gcfg.glyph_rows.iter() {
            for gmask in row.glyphs(&rgb) {
                let mut best_idx = 0;
                let mut best_score = 0.0;
                for (i, unique_gmask) in unique_glyphs.iter().enumerate() {
                    let score = gmask.score_similarity(&unique_gmask);
                    if score > best_score {
                        best_idx = i;
                        best_score = score;
                    }
                }

                let idx = if best_score >= GLYPH_MASK_SIMILARITY_THRESHOLD {
                    best_idx
                } else {
                    unique_glyphs.push(gmask.clone());
                    unique_glyphs.len() - 1
                };
                let path = output_dir.join(format!("glyph/{:02}/g_{:04}.bmp", idx, n_glyphs));
                n_glyphs += 1;
                std::fs::create_dir_all(path.parent().expect("path has parent"))?;
                gmask.bmp.save(path)?;
            }
        }

        info.set_progress(SetProgressInfo::detail(format!(
            "[dbg] glyphs exported for {:?}",
            clip.path
        )));
    }

    info.set_progress(SetProgressInfo::detail("[dbg] finished recognizing glyphs"));
    Ok(())
}
