#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Universe {
    size: u8,
    cells: Vec<u8>,
}

#[wasm_bindgen]
impl Universe {
    pub fn new(size: u8) -> Universe {
        return Universe {
            size,
            cells: vec![0; size.pow(2) as usize],
        };
    }
    pub fn new_stable_case_one() -> Universe {
        let size = (3 as u8).pow(2);
        let mut universe = Universe {
            size,
            cells: vec![1; size.pow(2) as usize],
        };
        universe.cells[4 as usize] = 10;
        return universe;
    }
    pub fn cells_pointer(&self) -> *const u8 {
        return self.cells.as_ptr();
    }
    pub fn seed(&mut self) {
        for cell_index in 0..self.size.pow(2) {
            if cell_index % 3 == 0 {
                self.cells[cell_index as usize] = 1;
            }
        }
    }
    pub fn tick(&mut self) {
        let mut next = self.cells.clone();
        for index in 0..(self.size - 1) {
            print!("[ tick ] {} => ", index);
            // for col in 0..self.width {
            //     let idx = self.get_index(row, col);
            //     let cell = self.cells[idx];
            //     let live_neighbors = self.live_neighbor_count(row, col);
            //     let next_cell = match (cell, live_neighbors) {
            //         // Rule 1: Any live cell with fewer than two live neighbours
            //         // dies, as if caused by underpopulation.
            //         (Cell::Alive, x) if x < 2 => Cell::Dead,
            //         // Rule 2: Any live cell with two or three live neighbours
            //         // lives on to the next generation.
            //         (Cell::Alive, 2) | (Cell::Alive, 3) => Cell::Alive,
            //         // Rule 3: Any live cell with more than three live
            //         // neighbours dies, as if by overpopulation.
            //         (Cell::Alive, x) if x > 3 => Cell::Dead,
            //         // Rule 4: Any dead cell with exactly three live neighbours
            //         // becomes a live cell, as if by reproduction.
            //         (Cell::Dead, 3) => Cell::Alive,
            //         // All other cells remain in the same state.
            //         (otherwise, _) => otherwise,
            //     };
            //     next[idx] = next_cell;
            // }
        }
        self.cells = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_inital_generation_no_panic() {
        Universe::new(10);
    }
    #[test]
    #[should_panic]
    #[allow(overflowing_literals)]
    fn test_out_of_bounds_input_panics() {
        Universe::new(100000);
    }
    #[test]
    fn test_seed_no_panic() {
        Universe::new(10).seed();
    }
    #[test]
    fn test_seed_tick_no_panic() {
        let mut universe = Universe::new(10);
        universe.seed();
        universe.tick();
    }
    #[test]
    fn test_seed_alters_data() {
        let mut universe = Universe::new(10);
        let cells_before = universe.cells.clone();
        universe.seed();
        let cells_after = universe.cells.clone();
        assert_ne!(cells_before, cells_after);
    }
    #[test]
    fn test_stable_case_one_no_panics() {
        Universe::new_stable_case_one();
    }
}
