//! A compact, decimated preview mesh for the browser viewer.
//!
//! Full slicer 3MFs can have millions of triangles and a model part bigger than 100 MB of XML.
//! The browser can't parse that without freezing, so the server reduces it with vertex
//! clustering and serves a few MB of positions, normals and indices instead.

use std::collections::{HashMap, HashSet};

use crate::model_thumbs::Tri;

/// `LMP1`: LiMa preview, version 1.
pub const MAGIC: [u8; 4] = *b"LMP1";
/// Triangle budget for one preview.
pub const TARGET_TRIANGLES: usize = 200_000;

#[derive(Debug, Clone, Default)]
pub struct PreviewMesh {
    /// `x, y, z` per vertex.
    pub positions: Vec<f32>,
    /// Unit normals, one per vertex.
    pub normals: Vec<f32>,
    /// Triangle indices into the vertices above.
    pub indices: Vec<u32>,
}

impl PreviewMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }
}

/// Reduces `tris` to about `max_triangles` with vertex clustering: nearby vertices become one,
/// then triangles that collapse are dropped. Fast, predictable, and good enough for an orbit preview.
pub fn build(tris: &[Tri], max_triangles: usize) -> PreviewMesh {
    let Some((min, max)) = bounds(tris) else {
        return PreviewMesh::default();
    };
    let target = max_triangles.max(1_000);
    let span = (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2]);
    let mut cell = if span.is_finite() && span > 0.0 { span / 96.0 } else { 1.0 };

    let mut best = cluster(tris, min, cell);
    // Coarsen until it fits the budget.
    for _ in 0..16 {
        if best.triangle_count() <= target {
            break;
        }
        cell *= 1.6;
        best = cluster(tris, min, cell);
    }
    // Spend the remaining budget on detail, but never cross the limit.
    for _ in 0..4 {
        if best.triangle_count() * 3 >= target {
            break;
        }
        let finer = cell / 1.5;
        let candidate = cluster(tris, min, finer);
        if candidate.triangle_count() > target {
            break;
        }
        best = candidate;
        cell = finer;
    }
    best
}

fn bounds(tris: &[Tri]) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for t in tris {
        for p in t {
            if !p.iter().all(|v| v.is_finite()) {
                continue;
            }
            any = true;
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
    }
    any.then_some((min, max))
}

fn cluster(tris: &[Tri], min: [f32; 3], cell: f32) -> PreviewMesh {
    let cell = cell.max(f32::EPSILON);
    let key_of = |p: [f32; 3]| {
        (
            ((p[0] - min[0]) / cell).floor() as i64,
            ((p[1] - min[1]) / cell).floor() as i64,
            ((p[2] - min[2]) / cell).floor() as i64,
        )
    };

    let mut cells: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut sum: Vec<[f64; 3]> = Vec::new();
    let mut normal_sum: Vec<[f64; 3]> = Vec::new();
    let mut count: Vec<u32> = Vec::new();
    let mut index: Vec<u32> = Vec::with_capacity(tris.len() * 3);
    let mut seen: HashSet<[u32; 3]> = HashSet::new();

    for t in tris {
        if !t.iter().flatten().all(|v| v.is_finite()) {
            continue;
        }
        let face = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        let ids = t.map(|p| {
            *cells.entry(key_of(p)).or_insert_with(|| {
                let id = sum.len() as u32;
                sum.push([0.0; 3]);
                normal_sum.push([0.0; 3]);
                count.push(0);
                id
            })
        });
        if ids[0] == ids[1] || ids[0] == ids[2] || ids[1] == ids[2] {
            continue;
        }
        for (i, id) in ids.iter().enumerate() {
            let id = *id as usize;
            for axis in 0..3 {
                sum[id][axis] += t[i][axis] as f64;
                normal_sum[id][axis] += face[axis] as f64;
            }
            count[id] += 1;
        }
        let mut dedupe = ids;
        dedupe.sort_unstable();
        if seen.insert(dedupe) {
            index.extend_from_slice(&ids);
        }
    }

    let mut positions = Vec::with_capacity(sum.len() * 3);
    let mut normals = Vec::with_capacity(sum.len() * 3);
    for (i, &c) in count.iter().enumerate() {
        let n = f64::from(c.max(1));
        for &axis in &sum[i] {
            positions.push((axis / n) as f32);
        }
        let normal = &normal_sum[i];
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if len > 1e-12 {
            for &axis in normal {
                normals.push((axis / len) as f32);
            }
        } else {
            normals.extend_from_slice(&[0.0, 0.0, 1.0]);
        }
    }

    PreviewMesh { positions, normals, indices: index }
}

/// Binary layout, little-endian: `LMP1`, vertex count `u32`, index count `u32`,
/// positions (`f32 × 3`), normals (`f32 × 3`), indices (`u32`).
pub fn encode(mesh: &PreviewMesh) -> Vec<u8> {
    let vertex_count = mesh.vertex_count() as u32;
    let index_count = mesh.indices.len() as u32;
    let mut out = Vec::with_capacity(12 + mesh.positions.len() * 8 + mesh.indices.len() * 4);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&vertex_count.to_le_bytes());
    out.extend_from_slice(&index_count.to_le_bytes());
    for value in &mesh.positions {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in &mesh.normals {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in &mesh.indices {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn grid(n: usize) -> Vec<Tri> {
        let mut tris = Vec::new();
        let p = |x: usize, y: usize| [x as f32, y as f32, ((x * 7 + y * 13) % 5) as f32];
        for y in 0..n {
            for x in 0..n {
                let (a, b, c, d) = (p(x, y), p(x + 1, y), p(x + 1, y + 1), p(x, y + 1));
                tris.push([a, b, c]);
                tris.push([a, c, d]);
            }
        }
        tris
    }

    #[test]
    fn keeps_a_small_mesh() {
        let mesh = build(&cube(), TARGET_TRIANGLES);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.vertex_count(), 8);
        for n in mesh.normals.chunks(3) {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-4, "normal length {len}");
        }
    }

    #[test]
    fn decimates_to_the_budget() {
        let mesh = build(&grid(150), 2_000);
        assert!(mesh.triangle_count() <= 2_000, "{} triangles", mesh.triangle_count());
        assert!(mesh.triangle_count() > 100);
        assert_eq!(mesh.indices.len() % 3, 0);
        assert_eq!(mesh.positions.len(), mesh.normals.len());
        assert!(mesh.positions.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn encodes_a_readable_header() {
        let mesh = build(&cube(), TARGET_TRIANGLES);
        let bytes = encode(&mesh);
        assert_eq!(&bytes[..4], &MAGIC);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize, mesh.vertex_count());
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize, mesh.indices.len());
        assert_eq!(bytes.len(), 12 + mesh.positions.len() * 4 + mesh.normals.len() * 4 + mesh.indices.len() * 4);
    }
}
