use crate::{invalid, Result};
use serde::{Deserialize, Serialize};

pub const SCENE: u32 = 1;
pub const MAX_PIXELS: u64 = 4_194_304;
pub const MAX_RAYS: u64 = 8_388_608;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub samples: u32,
}

impl Frame {
    pub fn validate(&self) -> Result<()> {
        let pixels = u64::from(self.width) * u64::from(self.height);
        if self.width == 0
            || self.height == 0
            || self.width > 4096
            || self.height > 4096
            || self.samples == 0
            || self.samples > 16
            || pixels > MAX_PIXELS
            || pixels * u64::from(self.samples) > MAX_RAYS
        {
            return Err(invalid("Frame exceeds limits. Use 1..4096 pixels per axis, 1..16 samples, at most 4194304 pixels and 8388608 rays."));
        }
        Ok(())
    }

    pub fn byte_len(&self) -> Result<usize> {
        self.validate()?;
        Ok(self.width as usize * self.height as usize * 4)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub scene: u32,
    pub tile: u32,
    pub frame: Frame,
    pub first_row: u32,
    pub rows: u32,
}

impl Job {
    pub fn validate(&self) -> Result<()> {
        self.frame.validate()?;
        if self.scene != SCENE
            || self.rows == 0
            || self
                .first_row
                .checked_add(self.rows)
                .is_none_or(|end| end > self.frame.height)
        {
            return Err(invalid("Invalid scene or tile bounds."));
        }
        Ok(())
    }

    pub fn byte_len(&self) -> Result<usize> {
        self.validate()?;
        Ok(self.frame.width as usize * self.rows as usize * 4)
    }
}

pub fn partition(frame: Frame, parts: usize) -> Result<Vec<Job>> {
    frame.validate()?;
    if parts == 0 || parts > 16 || parts > frame.height as usize {
        return Err(invalid(
            "Use 1..16 parts, with at least one image row per part.",
        ));
    }
    Ok((0..parts)
        .map(|i| {
            let start = frame.height as usize * i / parts;
            let end = frame.height as usize * (i + 1) / parts;
            Job {
                scene: SCENE,
                tile: i as u32,
                frame,
                first_row: start as u32,
                rows: (end - start) as u32,
            }
        })
        .collect())
}

#[derive(Debug)]
pub struct Tile {
    pub job: Job,
    pub pixels: Vec<u8>,
}

pub fn assemble(frame: Frame, tiles: Vec<Tile>) -> Result<Vec<u8>> {
    let mut image = vec![0; frame.byte_len()?];
    let mut seen_rows = vec![false; frame.height as usize];
    let mut seen_ids = std::collections::HashSet::new();
    for tile in tiles {
        let bytes = tile.job.byte_len()?;
        if tile.job.frame != frame || tile.pixels.len() != bytes || !seen_ids.insert(tile.job.tile)
        {
            return Err(invalid("Tile identity or length does not match the frame."));
        }
        let start = tile.job.first_row as usize;
        let end = start + tile.job.rows as usize;
        if seen_rows[start..end].iter().any(|seen| *seen) {
            return Err(invalid("Tiles overlap."));
        }
        seen_rows[start..end].fill(true);
        let offset = start * frame.width as usize * 4;
        image[offset..offset + bytes].copy_from_slice(&tile.pixels);
    }
    if seen_rows.iter().any(|seen| !seen) {
        return Err(invalid("Frame has missing rows."));
    }
    Ok(image)
}

#[derive(Clone, Copy)]
struct V(f64, f64, f64);
impl V {
    fn add(self, b: V) -> V {
        V(self.0 + b.0, self.1 + b.1, self.2 + b.2)
    }
    fn sub(self, b: V) -> V {
        V(self.0 - b.0, self.1 - b.1, self.2 - b.2)
    }
    fn scale(self, k: f64) -> V {
        V(self.0 * k, self.1 * k, self.2 * k)
    }
    fn dot(self, b: V) -> f64 {
        self.0 * b.0 + self.1 * b.1 + self.2 * b.2
    }
    fn unit(self) -> V {
        self.scale(1.0 / self.dot(self).sqrt())
    }
}

// Fixed CPU test scene. No executable, shader, file path, or command is accepted.
fn ray_color(direction: V, frame_id: u64) -> V {
    let shift = (frame_id % 17) as f64 * 0.015 - 0.12;
    let spheres = [
        (V(shift, 0.0, -1.5), 0.5, V(0.8, 0.2, 0.15)),
        (V(-0.8, 0.05, -2.0), 0.55, V(0.15, 0.35, 0.8)),
        (V(0.0, -100.5, -1.5), 100.0, V(0.45, 0.65, 0.3)),
    ];
    let mut closest = f64::INFINITY;
    let mut hit = None;
    for (center, radius, color) in spheres {
        let oc = center.scale(-1.0);
        let b = oc.dot(direction);
        let c = oc.dot(oc) - radius * radius;
        let d = b * b - c;
        if d >= 0.0 {
            let t = -b - d.sqrt();
            if t > 0.001 && t < closest {
                closest = t;
                let normal = direction.scale(t).sub(center).scale(1.0 / radius);
                let light = normal.dot(V(-0.6, 0.8, 0.5).unit()).max(0.0);
                hit = Some(color.scale(0.15 + 0.85 * light));
            }
        }
    }
    hit.unwrap_or_else(|| {
        let t = 0.5 * (direction.1 + 1.0);
        V(1.0, 1.0, 1.0)
            .scale(1.0 - t)
            .add(V(0.4, 0.65, 1.0).scale(t))
    })
}

pub fn render(job: Job) -> Result<Tile> {
    let mut pixels = Vec::with_capacity(job.byte_len()?);
    let frame = job.frame;
    let aspect = f64::from(frame.width) / f64::from(frame.height);
    for y in job.first_row..job.first_row + job.rows {
        for x in 0..frame.width {
            let mut color = V(0.0, 0.0, 0.0);
            for sample in 0..frame.samples {
                // Global coordinates keep samples independent of tile placement.
                let jitter_x = (f64::from(sample) + 0.5) / f64::from(frame.samples);
                let jitter_y =
                    (f64::from((sample * 7) % frame.samples) + 0.5) / f64::from(frame.samples);
                let u = (2.0 * (f64::from(x) + jitter_x) / f64::from(frame.width) - 1.0) * aspect;
                let v = 1.0 - 2.0 * (f64::from(y) + jitter_y) / f64::from(frame.height);
                color = color.add(ray_color(V(u, v, -1.0).unit(), frame.id));
            }
            color = color.scale(1.0 / f64::from(frame.samples));
            for value in [color.0, color.1, color.2] {
                pixels.push((value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
            pixels.push(255);
        }
    }
    Ok(Tile { job, pixels })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> Frame {
        Frame {
            id: 7,
            width: 17,
            height: 11,
            samples: 3,
        }
    }

    #[test]
    fn uneven_tiles_match_full_render() {
        let f = frame();
        let expected = render(partition(f, 1).unwrap()[0]).unwrap().pixels;
        for parts in 1..=11 {
            let mut tiles: Vec<_> = partition(f, parts)
                .unwrap()
                .into_iter()
                .map(|j| render(j).unwrap())
                .collect();
            tiles.reverse();
            assert_eq!(assemble(f, tiles).unwrap(), expected);
        }
    }
    #[test]
    fn reject_gaps_overlap_ids_and_wrong_frames() {
        let f = frame();
        let jobs = partition(f, 2).unwrap();
        assert!(assemble(f, vec![render(jobs[0]).unwrap()]).is_err());
        let mut overlap = jobs[1];
        overlap.first_row -= 1;
        assert!(assemble(f, vec![render(jobs[0]).unwrap(), render(overlap).unwrap()]).is_err());
        let mut duplicate = jobs[1];
        duplicate.tile = jobs[0].tile;
        assert!(assemble(
            f,
            vec![render(jobs[0]).unwrap(), render(duplicate).unwrap()]
        )
        .is_err());
        let mut wrong = jobs[1];
        wrong.frame.id += 1;
        assert!(assemble(f, vec![render(jobs[0]).unwrap(), render(wrong).unwrap()]).is_err());
    }
    #[test]
    fn bounds_are_checked_before_work() {
        for f in [
            Frame {
                width: u32::MAX,
                ..frame()
            },
            Frame {
                samples: u32::MAX,
                ..frame()
            },
            Frame {
                height: 0,
                ..frame()
            },
        ] {
            assert!(partition(f, 1).is_err());
        }
        let f = Frame {
            height: 1,
            ..frame()
        };
        assert!(partition(f, 1).is_ok());
        assert!(partition(f, 2).is_err());
        let mut job = partition(frame(), 1).unwrap()[0];
        job.first_row = u32::MAX;
        assert!(render(job).is_err());
    }
    #[test]
    fn sky_has_independent_expected_color() {
        // A vertical upward ray misses all spheres and reaches the blue sky endpoint.
        let c = ray_color(V(0.0, 1.0, 0.0), 0);
        assert_eq!([c.0, c.1, c.2], [0.4, 0.65, 1.0]);
    }
}
