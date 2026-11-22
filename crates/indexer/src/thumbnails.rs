//! JPEG thumbnails for image assets, stored under
//! `<thumbs_root>/<project_id>/<asset_id>.jpg` (served at `/media/thumbs`).
//! All functions are blocking: call them through `spawn_blocking`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader};

/// Longest side of a generated thumbnail, in pixels.
pub const THUMB_MAX_SIZE: u32 = 512;
const JPEG_QUALITY: u8 = 82;

pub fn thumb_path(thumbs_root: &Path, project_id: &str, asset_id: &str) -> PathBuf {
    thumbs_root.join(project_id).join(format!("{asset_id}.jpg"))
}

/// Decodes `source` and writes a downscaled JPEG to `dest`. The write is atomic
/// (temp file + rename) so a half written thumbnail is never served.
pub fn generate(source: &Path, dest: &Path) -> Result<(), String> {
    let mut decoder = ImageReader::open(source)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    // Phone photos are often stored sideways with an EXIF orientation tag.
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    img.apply_orientation(orientation);

    // Only downscale; small images are kept at their size.
    let thumb = if img.width() > THUMB_MAX_SIZE || img.height() > THUMB_MAX_SIZE {
        img.thumbnail(THUMB_MAX_SIZE, THUMB_MAX_SIZE)
    } else {
        img
    };
    // JPEG has no alpha channel.
    let rgb = thumb.to_rgb8();

    let dir = dest.parent().ok_or("Thumbnail path has no parent")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let tmp = dest.with_extension("jpg.tmp");
    let write = || -> Result<(), String> {
        let file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        let mut writer = io::BufWriter::new(file);
        JpegEncoder::new_with_quality(&mut writer, JPEG_QUALITY)
            .encode_image(&rgb)
            .map_err(|e| e.to_string())?;
        drop(writer);
        fs::rename(&tmp, dest).map_err(|e| e.to_string())
    };
    write().inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Removes an asset thumbnail. Missing files are not an error.
pub fn remove_asset(thumbs_root: &Path, project_id: &str, asset_id: &str) -> io::Result<()> {
    ignore_missing(fs::remove_file(thumb_path(thumbs_root, project_id, asset_id)))
}

/// Removes every thumbnail of a project. Missing folders are not an error.
pub fn remove_project(thumbs_root: &Path, project_id: &str) -> io::Result<()> {
    ignore_missing(fs::remove_dir_all(thumbs_root.join(project_id)))
}

fn ignore_missing(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_downscaled_jpeg() {
        let dir = std::env::temp_dir().join(format!("lima-thumb-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("big.png");
        image::RgbaImage::from_pixel(2000, 1000, image::Rgba([200, 10, 10, 128])).save(&src).unwrap();

        let dest = thumb_path(&dir.join("thumbs"), "p1", "a1");
        generate(&src, &dest).unwrap();

        let thumb = image::open(&dest).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (512, 256));
        assert!(!dest.with_extension("jpg.tmp").exists());

        remove_project(&dir.join("thumbs"), "p1").unwrap();
        assert!(!dest.exists());
        remove_project(&dir.join("thumbs"), "p1").unwrap(); // idempotent
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_non_images() {
        let dir = std::env::temp_dir().join(format!("lima-thumb-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("fake.jpg");
        fs::write(&src, "not an image").unwrap();
        assert!(generate(&src, &dir.join("out.jpg")).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
