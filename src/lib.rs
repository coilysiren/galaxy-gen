#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Universe {
    size: u16,
    cells: Vec<u16>,
}

// gas 0 - 99
// rock 100 - 9999
// star 10000 - 65536

#[wasm_bindgen]
impl Universe {
    pub fn new(size: u16) -> Universe {
        return Universe {
            size,
            cells: vec![0; size.pow(2) as usize],
        };
    }
    pub fn new_stable_case_one() -> Universe {
        let size = (3 as u16).pow(2);
        let mut universe = Universe {
            size,
            cells: vec![1; size.pow(2) as usize],
        };
        universe.cells[4 as usize] = 10;
        return universe;
    }
    pub fn cells_pointer(&self) -> *const u16 {
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
            let cell = self.cells[index as usize];
            let neighbours = self.neighbours(index, 1);
        }
        self.cells = next;
    }
}

impl Universe {
    fn neighbours(&self, index: u16, reach: u16) -> Vec<(u16, u16)> {
        let mut neighbours = Vec::new();
        let (index_row, index_col) = self.index_to_row_col(index);
        let (row_start, row_end) = self.reach_range(index_row, reach);
        let (col_start, col_end) = self.reach_range(index_col, reach);
        for row in row_start..=row_end {
            for col in col_start..=col_end {
                if (row, col) != (index_row, index_col) {
                    neighbours.push((row, col));
                }
            }
        }
        return neighbours;
    }
    fn row_col_to_index(&self, row: u16, col: u16) -> u16 {
        return row * self.size + col;
    }
    fn index_to_row_col(&self, index: u16) -> (u16, u16) {
        return (index / self.size, index % self.size);
    }
    fn reach_range(&self, index: u16, reach: u16) -> (u16, u16) {
        return (
            self.reach_range_start(index, reach),
            self.reach_range_end(index, reach),
        );
    }
    fn reach_range_start(&self, index: u16, reach: u16) -> u16 {
        let start;
        if index < reach {
            start = 0;
        } else {
            start = index - reach;
        }
        return start;
    }
    fn reach_range_end(&self, index: u16, reach: u16) -> u16 {
        let end;
        if index + reach > self.size {
            end = self.size - 1;
        } else {
            end = index + reach;
        }
        return end;
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

    #[test]
    fn test_index_to_row_col_start() {
        let universe = Universe::new(3);
        assert_eq!(universe.index_to_row_col(0), (0, 0));
    }
    #[test]
    fn test_row_col_to_index_start() {
        let universe = Universe::new(3);
        assert_eq!(universe.row_col_to_index(0, 0), 0);
    }

    #[test]
    fn test_index_to_row_col_center() {
        let universe = Universe::new(3);
        assert_eq!(universe.index_to_row_col(4), (1, 1));
    }
    #[test]
    fn test_row_col_to_index_center() {
        let universe = Universe::new(3);
        assert_eq!(universe.row_col_to_index(1, 1), 4);
    }

    #[test]
    fn test_index_to_row_col_end() {
        let universe = Universe::new(3);
        assert_eq!(universe.index_to_row_col(8), (2, 2));
    }
    #[test]
    fn test_row_col_to_index_end() {
        let universe = Universe::new(3);
        assert_eq!(universe.row_col_to_index(2, 2), 8);
    }

    #[test]
    fn test_index_edge_transform_top_right() {
        let universe = Universe::new(3);
        let index = 2;
        let (start, end) = universe.index_to_row_col(index);
        assert_eq!(universe.row_col_to_index(start, end), index);
    }
    #[test]
    fn test_index_edge_transform_bottom_left() {
        let universe = Universe::new(3);
        let index = 6;
        let (start, end) = universe.index_to_row_col(index);
        assert_eq!(universe.row_col_to_index(start, end), index);
    }

    #[test]
    fn test_reach_range_start_edge() {
        let universe = Universe::new(3);
        assert_eq!(universe.reach_range_start(0, 99), 0);
    }
    #[test]
    fn test_reach_range_start_overflow() {
        let universe = Universe::new(3);
        assert_eq!(universe.reach_range_start(1, 99), 0);
    }
    #[test]
    fn test_reach_range_start_contained() {
        let universe = Universe::new(10);
        assert_eq!(universe.reach_range_start(4, 2), 2);
    }

    #[test]
    fn test_reach_range_end_edge() {
        let universe = Universe::new(3);
        assert_eq!(universe.reach_range_end(2, 99), 2);
    }
    #[test]
    fn test_reach_range_end_overflow() {
        let universe = Universe::new(3);
        assert_eq!(universe.reach_range_end(0, 99), 2);
    }
    #[test]
    fn test_reach_range_end_contained() {
        let universe = Universe::new(10);
        assert_eq!(universe.reach_range_end(2, 2), 4);
    }

    #[test]
    fn test_neighbor_size() {
        let universe = Universe::new(10);
        assert_eq!(universe.neighbours(0, 1).len(), 3);
    }
    #[test]
    fn test_neighbor_size_larger() {
        let universe = Universe::new(10);
        assert_eq!(universe.neighbours(0, 2).len(), 8);
    }
    #[test]
    fn test_neighbor_size_center() {
        let universe = Universe::new(3);
        assert_eq!(universe.neighbours(4, 1).len(), 8);
    }

}
