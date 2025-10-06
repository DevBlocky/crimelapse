#[cfg(feature = "annotated-glyph-frames")]
mod annotate;
#[cfg(feature = "organized-glyph-bitmaps")]
mod organize;

use crate::{
    compute::{timeline::Timeline, workers::WorkerPool},
    ffmpeg, JobInfo, SetProgressInfo,
};
use anyhow::Context;
use image::{GenericImageView, GrayImage, Luma, Rgb, RgbImage, SubImage};
use regex::Regex;
use std::{path::Path, sync::Arc, time::Duration};

#[derive(Debug, Clone)]
struct GlyphMask {
    bmp: GrayImage,
}
impl GlyphMask {
    fn new(bmp: GrayImage) -> Self {
        Self { bmp }
    }
    fn score_similarity(&self, other: &Self) -> f64 {
        debug_assert_eq!(self.bmp.dimensions(), other.bmp.dimensions());

        let mut match_score = 0;
        let mut total_score = 0;
        for (&Luma([self_px]), &Luma([other_px])) in self.bmp.pixels().zip(other.bmp.pixels()) {
            // white pixels matching are worth 15x more than black pixels matching
            let score = if self_px > 127 || other_px > 127 {
                15
            } else {
                1
            };
            if self_px == other_px {
                match_score += score;
            }
            total_score += score;
        }

        match_score as f64 / total_score as f64
    }
}
impl<T: GenericImageView<Pixel = Rgb<u8>>> From<&T> for GlyphMask {
    fn from(value: &T) -> Self {
        const WHITE_AVG_MIN: u8 = 220;
        const WHITE_MAX_CHROMA: u8 = 30;

        let (width, height) = value.dimensions();
        let bmp = GrayImage::from_fn(width, height, |x, y| {
            let [r, g, b] = value.get_pixel(x, y).0;
            let avg = ((r as u16 + g as u16 + b as u16) / 3) as u8;
            let max_channel = r.max(g).max(b);
            let min_channel = r.min(g).min(b);
            let chroma = max_channel - min_channel;
            if avg >= WHITE_AVG_MIN && chroma <= WHITE_MAX_CHROMA {
                Luma([255])
            } else {
                Luma([0])
            }
        });

        Self { bmp }
    }
}

#[derive(Debug, serde::Deserialize)]
struct GlyphRow {
    top: u32,
    right: u32,
    width: u32,
    height: u32,
    columns: u32,
}
impl GlyphRow {
    fn crops<'a>(
        &self,
        img: &'a RgbImage,
    ) -> impl Iterator<Item = SubImage<&'a RgbImage>> + use<'a, '_> {
        (0..self.columns).map(|col| {
            let x = self.right + (col * self.width);
            let y = self.top;
            image::imageops::crop_imm(img, x, y, self.width, self.height)
        })
    }
    fn glyphs<'a>(&self, img: &'a RgbImage) -> impl Iterator<Item = GlyphMask> + use<'a, '_> {
        self.crops(img)
            .map(|crop| GlyphMask::from(&crop.to_image()))
    }
    fn scrape_string(&self, img: &RgbImage, chars: &[(String, GlyphMask)]) -> String {
        let mut s = String::with_capacity(self.columns as usize);
        for glyph in self.glyphs(&img) {
            let mut best_c = "";
            let mut best_score = 0.0;
            for (ref_c, ref_glyph) in chars {
                let score = glyph.score_similarity(ref_glyph);
                if score > best_score {
                    best_c = &ref_c;
                    best_score = score;
                }
            }

            s.push_str(best_c);
        }
        s
    }
}

#[derive(Debug, serde::Deserialize)]
struct GlyphChar {
    char: String,
    filepath: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlyphConfig {
    glyph_rows: Vec<GlyphRow>,
    glyph_chars: Vec<GlyphChar>,
}
impl GlyphConfig {
    fn from_resources(info: &JobInfo) -> anyhow::Result<Self> {
        let path = info.resolve_resource("resources/glyphconfig.json");
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        Ok(serde_json::from_reader(reader)?)
    }
    fn load_glyph_masks(&self, info: &JobInfo) -> anyhow::Result<Vec<(String, GlyphMask)>> {
        let mut char_masks = Vec::new();
        for gc in &self.glyph_chars {
            let path = info.resolve_resource(&gc.filepath);
            let img = image::open(path)?;
            char_masks.push((gc.char.clone(), GlyphMask::new(img.to_luma8())))
        }
        Ok(char_masks)
    }
}

#[derive(Debug, Default)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}
impl LatLng {
    fn from_strings(lat: &str, lng: &str) -> anyhow::Result<Self> {
        use std::sync::LazyLock;
        static LAT_REGEXP: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(N|S)[:. ](\d{2,3})[:. ](\d+)").expect("compile latitude regex")
        });
        static LNG_REGEXP: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(E|W)[:. ](\d{2,3})[:. ](\d+)").expect("compile longitude regex")
        });

        Ok(Self {
            lat: Self::parse_lat_lng(lat, &LAT_REGEXP).context("parse latitude")?,
            lng: Self::parse_lat_lng(lng, &LNG_REGEXP).context("parse longitude")?,
        })
    }
    fn parse_lat_lng(s: &str, r: &Regex) -> anyhow::Result<f64> {
        let (_, [cardinal, major, decimal]) = r
            .captures(s)
            .ok_or_else(|| anyhow::anyhow!("{} unmatched by regular expression {}", s, r))
            .context("match regular expression")?
            .extract();
        let val = format!("{}.{}", major, decimal)
            .parse::<f64>()
            .context("parse f64")?;
        match cardinal {
            "N" | "E" => Ok(val),
            "S" | "W" => Ok(-val),
            c => Err(anyhow::anyhow!("invalid cardinal {}", c)),
        }
    }
}
fn scrape_clip_location(
    info: &JobInfo,
    gcfg: &GlyphConfig,
    chars: &[(String, GlyphMask)],
    clip_path: &Path,
) -> anyhow::Result<LatLng> {
    info.cancel_result()?;

    let jpg_data = ffmpeg::extract_frame(clip_path, Duration::ZERO)?;
    let rgb = image::load_from_memory(&jpg_data)?.to_rgb8();
    std::mem::drop(jpg_data);

    let strings = gcfg
        .glyph_rows
        .iter()
        .map(|row| row.scrape_string(&rgb, &chars))
        .collect::<Vec<_>>();
    debug_assert_eq!(strings.len(), 2);

    let res = LatLng::from_strings(&strings[0], &strings[1]);
    let detail = match &res {
        Ok(_) => format!("scraped clip geolocation {:?}", clip_path),
        Err(e) => format!(
            "WARN: could not scrape clip geolocation {:?}\n{:?}\n\n",
            clip_path, e
        ),
    };
    info.set_progress(SetProgressInfo {
        progress_inc: Some(1),
        detail: Some(detail),
        ..Default::default()
    });
    Ok(res.unwrap_or_default())
}

pub fn scrape_locations(
    info: Arc<JobInfo>,
    timeline: Arc<Timeline>,
    pool: &WorkerPool,
    _output_dir: &Path,
) -> anyhow::Result<Vec<LatLng>> {
    let gcfg = Arc::new(GlyphConfig::from_resources(&info)?);

    // annotate frames = aligning/debugging the GlyphRows to timeline clip's thumbnail
    #[cfg(feature = "annotated-glyph-frames")]
    annotate::annotate_frames(&info, &timeline, &gcfg, _output_dir).context("annotate frames")?;
    // organize glyphs = extract glyphs from clips and export them (organizing by similarity)
    #[cfg(feature = "organized-glyph-bitmaps")]
    organize::organize_glyphs(&info, &timeline, &gcfg, _output_dir).context("recognize glyphs")?;

    let (timeline_len, _) = timeline.iter().size_hint();
    info.set_progress(SetProgressInfo {
        total: Some(timeline_len),
        progress: Some(0),
        ..Default::default()
    });

    let chars = Arc::new(gcfg.load_glyph_masks(&info).context("load glyph masks")?);
    let locations = pool.run_ordered_channel(timeline.iter().map(|clip| {
        let info = Arc::clone(&info);
        let gcfg = Arc::clone(&gcfg);
        let chars = Arc::clone(&chars);
        let clip_path = clip.path.clone();
        move || {
            scrape_clip_location(&info, &gcfg, &chars, &clip_path)
                .with_context(|| format!("scrape_clip_location for {:?}", clip_path))
        }
    }));

    let locations = locations.into_iter().collect::<anyhow::Result<_>>()?;
    info.set_progress(SetProgressInfo::detail("finished scraping geolocations"));
    Ok(locations)
}
