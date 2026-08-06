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
    NeutronStar = 2,
    Merging = 3,
    MergedRemnant = 4,
    RedGiant = 5,
    WhiteDwarf = 6,
}

impl Stage {
    pub fn from_u8(v: u8) -> Stage {
        match v {
            0 => Stage::MainSequence,
            1 => Stage::Remnant,
            2 => Stage::NeutronStar,
            3 => Stage::Merging,
            4 => Stage::MergedRemnant,
            5 => Stage::RedGiant,
            6 => Stage::WhiteDwarf,
            _ => Stage::MergedRemnant,
        }
    }
}

/// No-cluster sentinel for `cluster_id`.
pub const NO_CLUSTER: u32 = u32::MAX;
/// No compact-binary sentinel for `binary_id`.
pub const NO_BINARY: u32 = u32::MAX;

/// Floats per star in the flat serialization (see `to_flat`).
pub const STAR_FLOATS: usize = 15;

/// Floats per star in the render packing (see `render_data`).
pub const RENDER_FLOATS: usize = 6;

/// Struct-of-arrays star storage. Removal is swap-remove; indices are not
/// stable across ticks and must never be persisted outside a tick.
#[derive(Clone, Default)]
pub struct Stars {
    pub pos_x: Vec<f32>,
    pub pos_y: Vec<f32>,
    pub vel_x: Vec<f32>,
    pub vel_y: Vec<f32>,
    pub mass: Vec<f32>,
    /// Heavy-element mass carried by each star. This is a subset of mass.
    pub metal_mass: Vec<f32>,
    pub age: Vec<f32>,
    pub lifetime: Vec<f32>,
    pub stage: Vec<u8>,
    pub luminosity: Vec<f32>,
    pub color_index: Vec<f32>,
    pub cluster_id: Vec<u32>,
    /// Stable compact-binary identity. Paired core-collapse stars share it.
    pub binary_id: Vec<u32>,
    /// Consecutive halo-process scans spent beyond the phase-mixing radius.
    pub halo_dwell: Vec<u16>,
    /// Stable identity for event targeting - indices reorder on
    /// swap_remove. Fits f32 exactly below 2^24 spawns.
    pub id: Vec<u32>,
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
        metal_mass: f32,
        lifetime: f32,
        luminosity: f32,
        color_index: f32,
        cluster_id: u32,
        binary_id: u32,
        id: u32,
    ) -> usize {
        self.pos_x.push(pos_x);
        self.pos_y.push(pos_y);
        self.vel_x.push(vel_x);
        self.vel_y.push(vel_y);
        self.mass.push(mass);
        self.metal_mass.push(metal_mass.clamp(0.0, mass));
        self.age.push(0.0);
        self.lifetime.push(lifetime);
        self.stage.push(Stage::MainSequence as u8);
        self.luminosity.push(luminosity);
        self.color_index.push(color_index);
        self.cluster_id.push(cluster_id);
        self.binary_id.push(binary_id);
        self.halo_dwell.push(0);
        self.id.push(id);
        self.len() - 1
    }

    pub fn swap_remove(&mut self, i: usize) {
        self.pos_x.swap_remove(i);
        self.pos_y.swap_remove(i);
        self.vel_x.swap_remove(i);
        self.vel_y.swap_remove(i);
        self.mass.swap_remove(i);
        self.metal_mass.swap_remove(i);
        self.age.swap_remove(i);
        self.lifetime.swap_remove(i);
        self.stage.swap_remove(i);
        self.luminosity.swap_remove(i);
        self.color_index.swap_remove(i);
        self.cluster_id.swap_remove(i);
        self.binary_id.swap_remove(i);
        self.halo_dwell.swap_remove(i);
        self.id.swap_remove(i);
    }

    pub fn index_of_id(&self, id: u32) -> Option<usize> {
        self.id.iter().position(|&x| x == id)
    }

    /// Renderer packing: [x, y, luminosity, color_index, stage, cluster_id]
    /// per star. The renderer derives size and shared association glow from
    /// these values. Nothing flows back into the simulation.
    pub fn render_data(&self) -> Vec<f32> {
        let n = self.len();
        let mut out = Vec::with_capacity(n * RENDER_FLOATS);
        for i in 0..n {
            out.push(self.pos_x[i]);
            out.push(self.pos_y[i]);
            out.push(self.luminosity[i]);
            out.push(self.color_index[i]);
            out.push(self.stage[i] as f32);
            out.push(self.cluster_id[i] as f32);
        }
        out
    }

    /// Full flat serialization for the worker state round-trip. Layout per
    /// star: [x, y, vx, vy, mass, age, lifetime, stage, luminosity,
    /// color_index, cluster_id, binary_id, halo_dwell, id, metal_mass]. Integer ids
    /// survive f32 because live ids stay far below 2^24. The u32::MAX
    /// sentinels round-trip through Rust's saturating float cast.
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
            out.push(self.binary_id[i] as f32);
            out.push(self.halo_dwell[i] as f32);
            out.push(self.id[i] as f32);
            out.push(self.metal_mass[i]);
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
            s.binary_id.push(chunk[11] as u32);
            s.halo_dwell.push(chunk[12] as u16);
            s.id.push(chunk[13] as u32);
            s.metal_mass.push(chunk[14].clamp(0.0, chunk[4]));
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
        s.spawn(
            1.5, 2.5, -0.1, 0.2, 40.0, 0.8, 1000.0, 250.0, 0.7, 3, 7, 101,
        );
        s.spawn(
            9.0, 8.0, 0.3, -0.4, 12.0, 0.12, 4000.0, 40.0, 0.2, NO_CLUSTER, NO_BINARY, 102,
        );
        s.age[1] = 123.5;
        s.stage[0] = Stage::Remnant as u8;
        s.halo_dwell[0] = 4;
        let flat = s.to_flat();
        assert_eq!(flat.len(), 2 * STAR_FLOATS);
        let back = Stars::from_flat(&flat);
        assert_eq!(back.len(), 2);
        assert_eq!(back.pos_x, s.pos_x);
        assert_eq!(back.vel_y, s.vel_y);
        assert_eq!(back.age, s.age);
        assert_eq!(back.metal_mass, s.metal_mass);
        assert_eq!(back.stage, s.stage);
        assert_eq!(back.cluster_id, s.cluster_id);
        assert_eq!(back.binary_id, s.binary_id);
        assert_eq!(back.halo_dwell, s.halo_dwell);
        assert_eq!(back.id, s.id);
    }

    #[test]
    fn test_swap_remove_keeps_arrays_parallel() {
        let mut s = Stars::new();
        s.spawn(
            0.0, 0.0, 0.0, 0.0, 1.0, 0.01, 1.0, 1.0, 0.0, 0, NO_BINARY, 10,
        );
        s.spawn(
            1.0, 1.0, 0.0, 0.0, 2.0, 0.02, 1.0, 1.0, 0.0, 1, NO_BINARY, 11,
        );
        s.spawn(
            2.0, 2.0, 0.0, 0.0, 3.0, 0.03, 1.0, 1.0, 0.0, 2, NO_BINARY, 12,
        );
        s.swap_remove(0);
        assert_eq!(s.len(), 2);
        assert_eq!(s.pos_x[0], 2.0);
        assert_eq!(s.mass[0], 3.0);
        assert_eq!(s.metal_mass[0], 0.03);
        assert_eq!(s.cluster_id[0], 2);
        assert_eq!(s.index_of_id(11), Some(1));
        assert_eq!(s.index_of_id(10), None);
    }
}
