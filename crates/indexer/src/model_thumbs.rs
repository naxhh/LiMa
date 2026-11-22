//! Preview images for model files: the picture a slicer embedded in a 3MF, or else a
//! shaded render of the mesh (STL, OBJ, 3MF). Blocking: call through `spawn_blocking`.

use std::fs;
use std::io::Read;
use std::path::Path;

use image::{DynamicImage, Rgb, RgbImage, imageops};

/// Model extensions that get a preview.
pub const MODEL_EXTENSIONS: &[&str] = &["3mf", "stl", "obj"];

/// Meshes bigger than this aren't rendered (memory and time); embedded 3MF thumbnails still work.
const MAX_RENDER_BYTES: u64 = 256 * 1024 * 1024;
/// Rendering more triangles than this samples them; the preview is 512 px wide anyway.
const MAX_TRIANGLES: usize = 3_000_000;

const WIDTH: u32 = 512;
const HEIGHT: u32 = 384;
/// Supersampling factor for anti-aliasing.
const SS: u32 = 2;
const BACKGROUND: [u8; 3] = [26, 31, 39];
const MODEL: [f32; 3] = [159.0, 180.0, 199.0];

type Tri = [[f32; 3]; 3];

pub fn is_model(path: &Path) -> bool {
    extension(path).is_some_and(|e| MODEL_EXTENSIONS.contains(&e.as_str()))
}

fn extension(path: &Path) -> Option<String> {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
}

/// The preview for `source`: an embedded 3MF thumbnail if there is one, otherwise a render.
pub fn preview(source: &Path) -> Result<DynamicImage, String> {
    let ext = extension(source).unwrap_or_default();
    let size = fs::metadata(source).map_err(|e| e.to_string())?.len();
    if ext == "3mf" {
        if let Some(img) = embedded_3mf_thumbnail(source)? {
            return Ok(img);
        }
    }
    if size > MAX_RENDER_BYTES {
        return Err(format!("too large to render ({} MB)", size / (1024 * 1024)));
    }
    let tris = match ext.as_str() {
        "stl" => parse_stl(&fs::read(source).map_err(|e| e.to_string())?)?,
        "obj" => parse_obj(&fs::read_to_string(source).map_err(|e| e.to_string())?),
        "3mf" => mesh_3mf(source)?,
        other => return Err(format!("unsupported model format {other:?}")),
    };
    render(&tris).map(DynamicImage::ImageRgb8).ok_or_else(|| "no triangles to render".to_string())
}

// ---------- 3MF ----------

/// Thumbnail paths used by Bambu Studio, OrcaSlicer, PrusaSlicer and the 3MF spec, best first.
const THUMBNAIL_NAMES: &[&str] = &[
    "metadata/plate_1.png",
    "metadata/thumbnail.png",
    "metadata/top_1.png",
    "auxiliaries/.thumbnails/thumbnail_middle.png",
    "auxiliaries/.thumbnails/thumbnail_3mf.png",
];

fn open_zip(source: &Path) -> Result<zip::ZipArchive<fs::File>, String> {
    zip::ZipArchive::new(fs::File::open(source).map_err(|e| e.to_string())?).map_err(|e| format!("not a valid 3MF: {e}"))
}

fn entry_names(zip: &zip::ZipArchive<fs::File>) -> Vec<String> {
    zip.file_names().filter_map(|n| n.ok()).map(|n| n.to_string()).collect()
}

fn embedded_3mf_thumbnail(source: &Path) -> Result<Option<DynamicImage>, String> {
    let mut zip = open_zip(source)?;
    let names = entry_names(&zip);
    let pick = THUMBNAIL_NAMES
        .iter()
        .find_map(|want| names.iter().find(|n| n.to_ascii_lowercase() == *want))
        // Any other PNG the slicer left in Metadata/ or a thumbnails folder.
        .or_else(|| {
            names.iter().find(|n| {
                let l = n.to_ascii_lowercase();
                l.ends_with(".png") && (l.starts_with("metadata/") || l.contains(".thumbnails/")) && !l.contains("pick_") && !l.contains("_small")
            })
        })
        .cloned();
    let Some(name) = pick else { return Ok(None) };
    let mut bytes = Vec::new();
    zip.by_name(&name).map_err(|e| e.to_string())?.take(64 * 1024 * 1024).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(image::load_from_memory(&bytes).ok())
}

/// Every mesh in the 3MF's `.model` parts. Build transforms are ignored: good enough for a preview.
fn mesh_3mf(source: &Path) -> Result<Vec<Tri>, String> {
    let mut zip = open_zip(source)?;
    let parts: Vec<String> = entry_names(&zip).into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".model")).collect();
    let mut tris = Vec::new();
    for part in parts {
        let mut xml = String::new();
        zip.by_name(&part)
            .map_err(|e| e.to_string())?
            .take(MAX_RENDER_BYTES * 4)
            .read_to_string(&mut xml)
            .map_err(|e| e.to_string())?;
        parse_3mf_model(&xml, &mut tris);
    }
    Ok(tris)
}

/// Reads `<vertex x y z>` and `<triangle v1 v2 v3>` elements. Vertex indices are per `<mesh>`.
fn parse_3mf_model(xml: &str, tris: &mut Vec<Tri>) {
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut base = 0usize;
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else { break };
        let tag = &after[..end];
        rest = &after[end + 1..];
        let name = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        let local = name.rsplit(':').next().unwrap_or(name);
        match local {
            "mesh" => base = vertices.len(),
            "vertex" => {
                if let (Some(x), Some(y), Some(z)) = (attr_f32(tag, "x"), attr_f32(tag, "y"), attr_f32(tag, "z")) {
                    vertices.push([x, y, z]);
                }
            }
            "triangle" => {
                let idx = |k| attr_f32(tag, k).map(|v| base + v as usize);
                if let (Some(a), Some(b), Some(c)) = (idx("v1"), idx("v2"), idx("v3")) {
                    if let (Some(a), Some(b), Some(c)) = (vertices.get(a), vertices.get(b), vertices.get(c)) {
                        tris.push([*a, *b, *c]);
                    }
                }
            }
            _ => {}
        }
    }
}

fn attr_f32(tag: &str, key: &str) -> Option<f32> {
    let mut search = tag;
    while let Some(pos) = search.find(key) {
        let before_ok = pos == 0 || search.as_bytes()[pos - 1].is_ascii_whitespace();
        let after = search[pos + key.len()..].trim_start();
        if before_ok {
            if let Some(after) = after.strip_prefix('=') {
                let after = after.trim_start();
                let quote = after.chars().next()?;
                if quote == '"' || quote == '\'' {
                    let value = &after[1..];
                    return value[..value.find(quote)?].trim().parse().ok();
                }
            }
        }
        search = &search[pos + key.len()..];
    }
    None
}

// ---------- STL / OBJ ----------

fn parse_stl(bytes: &[u8]) -> Result<Vec<Tri>, String> {
    if bytes.len() >= 84 {
        let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        // Binary when the size matches exactly (some binary files start with "solid" too).
        if 84 + n * 50 == bytes.len() {
            let f = |o: usize| f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
            return Ok((0..n)
                .map(|i| {
                    let o = 84 + i * 50 + 12;
                    [[f(o), f(o + 4), f(o + 8)], [f(o + 12), f(o + 16), f(o + 20)], [f(o + 24), f(o + 28), f(o + 32)]]
                })
                .collect());
        }
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "not a valid STL".to_string())?;
    let mut verts = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        if it.next() == Some("vertex") {
            let v: Vec<f32> = it.filter_map(|s| s.parse().ok()).collect();
            if v.len() == 3 {
                verts.push([v[0], v[1], v[2]]);
            }
        }
    }
    Ok(verts.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect())
}

/// OBJ is Y-up: vertices are turned Z-up like the other formats.
fn parse_obj(text: &str) -> Vec<Tri> {
    let mut verts: Vec<[f32; 3]> = Vec::new();
    let mut tris = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let v: Vec<f32> = it.take(3).filter_map(|s| s.parse().ok()).collect();
                if v.len() == 3 {
                    verts.push([v[0], -v[2], v[1]]);
                }
            }
            Some("f") => {
                let idx: Vec<usize> = it
                    .filter_map(|s| s.split('/').next()?.parse::<i64>().ok())
                    .filter_map(|i| if i < 0 { verts.len().checked_sub(i.unsigned_abs() as usize) } else { (i as usize).checked_sub(1) })
                    .collect();
                // Polygons are fanned into triangles.
                for k in 1..idx.len().saturating_sub(1) {
                    if let (Some(a), Some(b), Some(c)) = (verts.get(idx[0]), verts.get(idx[k]), verts.get(idx[k + 1])) {
                        tris.push([*a, *b, *c]);
                    }
                }
            }
            _ => {}
        }
    }
    tris
}

// ---------- Rendering ----------

/// Shaded isometric-style view of a Z-up mesh, with a depth buffer and supersampling.
fn render(tris: &[Tri]) -> Option<RgbImage> {
    if tris.is_empty() {
        return None;
    }
    let stride = tris.len().div_ceil(MAX_TRIANGLES).max(1);
    let (yaw, elev) = (-45f32.to_radians(), 30f32.to_radians());
    let (cy, sy, ce, se) = (yaw.cos(), yaw.sin(), elev.cos(), elev.sin());
    // Screen x, screen up, depth (larger = farther).
    let view = |p: [f32; 3]| {
        let x = p[0] * cy - p[1] * sy;
        let r = p[0] * sy + p[1] * cy;
        [x, p[2] * ce + r * se, r * ce - p[2] * se]
    };
    let projected: Vec<[[f32; 3]; 3]> = tris
        .iter()
        .step_by(stride)
        .map(|t| [view(t[0]), view(t[1]), view(t[2])])
        .filter(|t| t.iter().flatten().all(|v| v.is_finite()))
        .collect();
    if projected.is_empty() {
        return None;
    }

    let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in projected.iter().flatten() {
        min = [min[0].min(p[0]), min[1].min(p[1])];
        max = [max[0].max(p[0]), max[1].max(p[1])];
    }
    let (w, h) = ((WIDTH * SS) as f32, (HEIGHT * SS) as f32);
    let span = [(max[0] - min[0]).max(1e-6), (max[1] - min[1]).max(1e-6)];
    let scale = (w * 0.84 / span[0]).min(h * 0.84 / span[1]);
    let (cx, cyy) = ((min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0);
    let to_px = |p: [f32; 3]| [w / 2.0 + (p[0] - cx) * scale, h / 2.0 - (p[1] - cyy) * scale, p[2]];

    let (wi, hi) = (w as usize, h as usize);
    let mut depth = vec![f32::INFINITY; wi * hi];
    let mut img = RgbImage::from_pixel(w as u32, h as u32, Rgb(BACKGROUND));
    // Light from the upper left, towards the viewer.
    let light = normalize([-0.45, 0.7, -0.55]);

    for t in &projected {
        let n = normalize(cross(sub(t[1], t[0]), sub(t[2], t[0])));
        if !n.iter().all(|v| v.is_finite()) {
            continue;
        }
        let shade = 0.28 + 0.72 * dot(n, light).abs();
        let color = Rgb(MODEL.map(|c| (c * shade).min(255.0) as u8));

        let [a, b, c] = [to_px(t[0]), to_px(t[1]), to_px(t[2])];
        let area = edge(a, b, c);
        if area.abs() < 1e-9 {
            continue;
        }
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(wi.saturating_sub(1));
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let y1 = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(hi.saturating_sub(1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = [x as f32 + 0.5, y as f32 + 0.5, 0.0];
                let (w0, w1, w2) = (edge(b, c, p) / area, edge(c, a, p) / area, edge(a, b, p) / area);
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let i = y * wi + x;
                if z < depth[i] {
                    depth[i] = z;
                    img.put_pixel(x as u32, y as u32, color);
                }
            }
        }
    }
    Some(imageops::resize(&img, WIDTH, HEIGHT, imageops::FilterType::Triangle))
}

fn edge(a: [f32; 3], b: [f32; 3], p: [f32; 3]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A unit cube as 12 triangles.
    fn cube() -> Vec<Tri> {
        let v = |x: f32, y: f32, z: f32| [x * 10.0, y * 10.0, z * 10.0];
        let q = |a, b, c, d| [[a, b, c], [a, c, d]];
        [
            q(v(0., 0., 0.), v(1., 0., 0.), v(1., 1., 0.), v(0., 1., 0.)),
            q(v(0., 0., 1.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 1.)),
            q(v(0., 0., 0.), v(1., 0., 0.), v(1., 0., 1.), v(0., 0., 1.)),
            q(v(0., 1., 0.), v(1., 1., 0.), v(1., 1., 1.), v(0., 1., 1.)),
            q(v(0., 0., 0.), v(0., 1., 0.), v(0., 1., 1.), v(0., 0., 1.)),
            q(v(1., 0., 0.), v(1., 1., 0.), v(1., 1., 1.), v(1., 0., 1.)),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn binary_stl(tris: &[Tri]) -> Vec<u8> {
        let mut out = vec![0u8; 80];
        out.extend((tris.len() as u32).to_le_bytes());
        for t in tris {
            out.extend([0u8; 12]);
            for v in t {
                for c in v {
                    out.extend(c.to_le_bytes());
                }
            }
            out.extend([0u8; 2]);
        }
        out
    }

    fn tmp() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lima-model-thumb-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Fraction of pixels that aren't background.
    fn coverage(img: &DynamicImage) -> f32 {
        let rgb = img.to_rgb8();
        let drawn = rgb.pixels().filter(|p| p.0.iter().zip(BACKGROUND).any(|(a, b)| a.abs_diff(b) > 8)).count();
        drawn as f32 / (rgb.width() * rgb.height()) as f32
    }

    #[test]
    fn renders_binary_and_ascii_stl_and_obj() {
        let dir = tmp();
        let bin = dir.join("cube.stl");
        fs::write(&bin, binary_stl(&cube())).unwrap();
        let img = preview(&bin).unwrap();
        assert_eq!((img.width(), img.height()), (WIDTH, HEIGHT));
        let c = coverage(&img);
        assert!(c > 0.15 && c < 0.9, "cube should fill a good part of the frame, got {c}");

        let mut ascii = String::from("solid cube\n");
        for t in cube() {
            ascii.push_str("facet normal 0 0 0\nouter loop\n");
            for v in t {
                ascii.push_str(&format!("vertex {} {} {}\n", v[0], v[1], v[2]));
            }
            ascii.push_str("endloop\nendfacet\n");
        }
        let asc = dir.join("cube-ascii.stl");
        fs::write(&asc, ascii).unwrap();
        assert!(coverage(&preview(&asc).unwrap()) > 0.15);

        let obj = dir.join("quad.obj");
        fs::write(&obj, "v 0 0 0\nv 10 0 0\nv 10 10 0\nv 0 10 0\nv 0 0 10\nf 1/1/1 2/2/2 3/3/3 4/4/4\nf -5 -4 -1\n").unwrap();
        assert_eq!(parse_obj(&fs::read_to_string(&obj).unwrap()).len(), 3, "quad fanned into 2 + one negative-index triangle");
        assert!(coverage(&preview(&obj).unwrap()) > 0.05);
        fs::remove_dir_all(dir).unwrap();
    }

    fn write_3mf(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn prefers_embedded_3mf_thumbnail_else_renders_the_mesh() {
        let dir = tmp();
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(RgbImage::from_pixel(300, 300, Rgb([250, 0, 0])))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let model = br#"<?xml version="1.0"?><model xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources>
            <object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x='10' y='0' z='0'/><vertex y="10" x="0" z="0"/><vertex x="0" y="0" z="10"/></vertices>
            <triangles><triangle v1="0" v2="1" v3="2"/><triangle v1="0" v2="1" v3="3"/><triangle v1="0" v2="2" v3="3"/><triangle v1="1" v2="2" v3="3"/></triangles></mesh></object>
            </resources></model>"#;

        let with_thumb = dir.join("plate.3mf");
        write_3mf(&with_thumb, &[("3D/3dmodel.model", model), ("Metadata/plate_1.png", &png)]);
        let img = preview(&with_thumb).unwrap().to_rgb8();
        assert_eq!(img.get_pixel(150, 150).0, [250, 0, 0], "embedded thumbnail is used as is");

        let mesh_only = dir.join("cad.3mf");
        write_3mf(&mesh_only, &[("3D/3dmodel.model", model)]);
        let mut tris = Vec::new();
        parse_3mf_model(std::str::from_utf8(model).unwrap(), &mut tris);
        assert_eq!(tris.len(), 4, "attributes in any order and quote style");
        assert!(coverage(&preview(&mesh_only).unwrap()) > 0.05);

        let broken = dir.join("broken.3mf");
        fs::write(&broken, "not a zip").unwrap();
        assert!(preview(&broken).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
