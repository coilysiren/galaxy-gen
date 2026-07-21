//! Sparse star population. Stars are collisionless points with continuous
//! positions - they read the coarse gravity field and never occupy grid
//! cells, so they cannot jam and orbit honestly at any speed. See
//! docs/processes-events.md.

/// Lifecycle stage. A u8 enum plus data, deliberately not a trait object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Stage {
    MainSequence = 0,
    Remnant = 1,
}

impl Stage {
    pub fn from_u8(v: u8) -> Stage {
        match v {
            0 => Stage::MainSequence,
            _ => Stage::Remnant,
        }
    }
}

/// No-cluster sentinel for `cluster_id`.
pub const NO_CLUSTER: u32 = u32::MAX;

/// Floats per star in the flat serialization (see `to_flat`).
pub const STAR_FLOATS: usize = 11;

/// Floats per star in the render packing (see `render_data`).
pub const RENDER_FLOATS: usize = 4;

/// Struct-of-arrays star storage. Removal is swap-remove; indices are not
/// stable across ticks and must never be persisted outside a tick.
#[derive(Clone, Default)]
pub struct Stars {
    pub pos_x: Vec<f32>,
    pub pos_y: Vec<f32>,
    pub vel_x: Vec<f32>,
    pub vel_y: Vec<f32>,
    pub mass: Vec<f32>,
    pub age: Vec<f32>,
    pub lifetime: Vec<f32>,
    pub stage: Vec<u8>,
    pub luminosity: Vec<f32>,
    pub color_index: Vec<f32>,
    pub cluster_id: Vec<u32>,
}

impl Stars {
    pub fn new() -> Stars {
        Stars::default()
    }

    pub fn len(&self) -> usize {
        self.pos_x.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pos_x.is_empty()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        pos_x: f32,
        pos_y: f32,
        vel_x: f32,
        vel_y: f32,
        mass: f32,
        lifetime: f32,
        luminosity: f32,
        color_index: f32,
        cluster_id: u32,
    ) -> usize {
        self.pos_x.push(pos_x);
        self.pos_y.push(pos_y);
        self.vel_x.push(vel_x);
        self.vel_y.push(vel_y);
        self.mass.push(mass);
        self.age.push(0.0);
        self.lifetime.push(lifetime);
        self.stage.push(Stage::MainSequence as u8);
        self.luminosity.push(luminosity);
        self.color_index.push(color_index);
        self.cluster_id.push(cluster_id);
        self.len() - 1
    }

    pub fn swap_remove(&mut self, i: usize) {
        self.pos_x.swap_remove(i);
        self.pos_y.swap_remove(i);
        self.vel_x.swap_remove(i);
        self.vel_y.swap_remove(i);
        self.mass.swap_remove(i);
        self.age.swap_remove(i);
        self.lifetime.swap_remove(i);
        self.stage.swap_remove(i);
        self.luminosity.swap_remove(i);
        self.color_index.swap_remove(i);
        self.cluster_id.swap_remove(i);
    }

    /// Renderer packing: [x, y, luminosity, color_index] per star. The
    /// renderer derives size/glow from these; nothing flows back.
    pub fn render_data(&self) -> Vec<f32> {
        let n = self.len();
        let mut out = Vec::with_capacity(n * RENDER_FLOATS);
        for i in 0..n {
            out.push(self.pos_x[i]);
            out.push(self.pos_y[i]);
            out.push(self.luminosity[i]);
            out.push(self.color_index[i]);
        }
        out
    }

    /// Full flat serialization for the worker state round-trip. Layout per
    /// star: [x, y, vx, vy, mass, age, lifetime, stage, luminosity,
    /// color_index, cluster_id]. cluster_id survives f32 because ids stay
    /// far below 2^24.
    pub fn to_flat(&self) -> Vec<f32> {
        let n = self.len();
        let mut out = Vec::with_capacity(n * STAR_FLOATS);
        for i in 0..n {
            out.push(self.pos_x[i]);
            out.push(self.pos_y[i]);
            out.push(self.vel_x[i]);
            out.push(self.vel_y[i]);
            out.push(self.mass[i]);
            out.push(self.age[i]);
            out.push(self.lifetime[i]);
            out.push(self.stage[i] as f32);
            out.push(self.luminosity[i]);
            out.push(self.color_index[i]);
            out.push(self.cluster_id[i] as f32);
        }
        out
    }

    pub fn from_flat(data: &[f32]) -> Stars {
        let mut s = Stars::new();
        for chunk in data.chunks_exact(STAR_FLOATS) {
            s.pos_x.push(chunk[0]);
            s.pos_y.push(chunk[1]);
            s.vel_x.push(chunk[2]);
            s.vel_y.push(chunk[3]);
            s.mass.push(chunk[4]);
            s.age.push(chunk[5]);
            s.lifetime.push(chunk[6]);
            s.stage.push(chunk[7] as u8);
            s.luminosity.push(chunk[8]);
            s.color_index.push(chunk[9]);
            s.cluster_id.push(chunk[10] as u32);
        }
        s
    }
}

#[cfg(test)]
mod tests_stars {
    use super::*;

    #[test]
    fn test_flat_round_trip_is_exact() {
        let mut s = Stars::new();
        s.spawn(1.5, 2.5, -0.1, 0.2, 40.0, 1000.0, 250.0, 0.7, 3);
        s.spawn(9.0, 8.0, 0.3, -0.4, 12.0, 4000.0, 40.0, 0.2, NO_CLUSTER);
        s.age[1] = 123.5;
        s.stage[0] = Stage::Remnant as u8;
        let flat = s.to_flat();
        assert_eq!(flat.len(), 2 * STAR_FLOATS);
        let back = Stars::from_flat(&flat);
        assert_eq!(back.len(), 2);
        assert_eq!(back.pos_x, s.pos_x);
        assert_eq!(back.vel_y, s.vel_y);
        assert_eq!(back.age, s.age);
        assert_eq!(back.stage, s.stage);
        assert_eq!(back.cluster_id, s.cluster_id);
    }

    #[test]
    fn test_swap_remove_keeps_arrays_parallel() {
        let mut s = Stars::new();
        s.spawn(0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0);
        s.spawn(1.0, 1.0, 0.0, 0.0, 2.0, 1.0, 1.0, 0.0, 1);
        s.spawn(2.0, 2.0, 0.0, 0.0, 3.0, 1.0, 1.0, 0.0, 2);
        s.swap_remove(0);
        assert_eq!(s.len(), 2);
        assert_eq!(s.pos_x[0], 2.0);
        assert_eq!(s.mass[0], 3.0);
        assert_eq!(s.cluster_id[0], 2);
    }
}
