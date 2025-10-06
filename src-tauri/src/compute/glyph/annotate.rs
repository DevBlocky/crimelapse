use std::{path::Path, time::Duration};

use crate::{compute::{glyph::GlyphConfig, timeline::Timeline}, ffmpeg, JobInfo, SetProgressInfo};

use anyhow::Context;
use image::{Rgb, RgbImage};

#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn draw_rect_outline(img: &mut RgbImage, rect: Rect, color: Rgb<u8>) {
    let img_w = img.width();
    let img_h = img.height();

    let x0 = rect.x.clamp(0, img_w - 1);
    let y0 = rect.y.clamp(0, img_h - 1);
    let x1 = (rect.x + rect.width - 1).clamp(0, img_w - 1);
    let y1 = (rect.y + rect.height - 1).clamp(0, img_h - 1);

    if x0 > x1 || y0 > y1 {
        return;
    }

    for x in x0..=x1 {
        img.put_pixel(x, y0, color);
        img.put_pixel(x, y1, color);
    }

    for y in y0..=y1 {
        img.put_pixel(x0, y, color);
        img.put_pixel(x1, y, color);
    }
}

fn annotate_image(img: &mut RgbImage, gcfg: &GlyphConfig) {
    const OUTLINE_COLOR: Rgb<u8> = Rgb([255, 0, 0]);

    for grow in &gcfg.glyph_rows {
        for col in 0..grow.columns {
            let rect = Rect {
                x: grow.right + (col * grow.width),
                y: grow.top,
                width: grow.width,
                height: grow.height,
            };
            draw_rect_outline(img, rect, OUTLINE_COLOR);
        }
    }
}

pub fn annotate_frames(
    info: &JobInfo,
    timeline: &Timeline,
    gcfg: &GlyphConfig,
    output_dir: &Path,
) -> anyhow::Result<()> {
    let output_dir = output_dir.join("glyph");
    std::fs::create_dir_all(&output_dir)?;

    info.set_progress(SetProgressInfo::detail("[dbg] annotating frames"));
    for (i, clip) in timeline.iter().enumerate() {
        info.cancel_result()?;

        let jpg_data =
            ffmpeg::extract_frame(&clip.path, Duration::ZERO).context("load jpg data")?;
        let mut rgb = image::load_from_memory(&jpg_data)
            .context("load dynamic image")?
            .to_rgb8();
        std::mem::drop(jpg_data);
        annotate_image(&mut rgb, &gcfg);

        let output_path = output_dir.join(format!("{:04}.jpg", i));
        image::DynamicImage::ImageRgb8(rgb)
            .save(&output_path)
            .with_context(|| {
                format!(
                    "save debug annoted glyph frame to {}",
                    output_path.display()
                )
            })?;

        info.set_progress(SetProgressInfo::detail(format!(
            "[dbg] annotated glyph frame exported to {}",
            output_path.display()
        )));
    }
    Ok(())
}
