#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    mass: u16,
}

#[wasm_bindgen]
impl Cell {
    pub fn get_type(&self) -> u8 {
        if self.mass < 100 {
            return 0;
        } else if self.mass < 10000 {
            return 1;
        } else {
            return 2;
        }
    }
    pub fn is_gas(&self) -> bool {
        return self.check_if_type(0);
    }
    pub fn is_rock(&self) -> bool {
        return self.check_if_type(1);
    }
    pub fn is_star(&self) -> bool {
        return self.check_if_type(2);
    }
}

impl Cell {
    fn check_if_type(&self, type_index: u8) -> bool {
        if (type_index == 0) & (self.mass < 100) {
            return true;
        } else if (type_index == 1) & (self.mass >= 100) & (self.mass < 10000) {
            return true;
        } else if (type_index == 2) & (self.mass >= 10000) {
            return true;
        } else {
            return false;
        }
    }
}

#[wasm_bindgen]
pub struct Universe {
    size: u16,
    cells: Vec<Cell>,
}

#[wasm_bindgen]
impl Universe {
    pub fn new(size: u16) -> Universe {
        return Universe {
            size,
            cells: vec![Cell { mass: 0 }; (size as u32).pow(2) as usize],
        };
    }
    pub fn new_stable_case_one() -> Universe {
        let size = (3 as u16).pow(2);
        let mut universe = Universe {
            size,
            cells: vec![Cell { mass: 1 }; size.pow(2) as usize],
        };
        universe.cells[4 as usize] = Cell { mass: 10 };
        return universe;
    }
    pub fn cells_pointer(&self) -> *const Cell {
        return self.cells.as_ptr();
    }
    pub fn seed(&mut self) {
        for cell_index in 0..self.size.pow(2) {
            if cell_index % 3 == 0 {
                self.cells[cell_index as usize] = Cell { mass: 1 };
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
    fn reach_of_type(&self, type_index: u8) -> u16 {
        match type_index {
            0 => self.size / 100 + 1,
            1 => self.size / 10 + 1,
            2 => self.size,
            _ => unreachable!(),
        }
    }
    fn neighbours_of_my_type(&self, index: u16) -> Vec<(u16, u16)> {
        return self.neighbours_of_type(index, self.cells[index as usize].get_type());
    }
    fn neighbours_of_type(&self, index: u16, type_index: u8) -> Vec<(u16, u16)> {
        let mut neighbours_of_type = Vec::new();
        for neighbour in self.neighbours(index, self.reach_of_type(type_index)) {
            if self.cells[index as usize].check_if_type(type_index) {
                neighbours_of_type.push(neighbour);
            }
        }
        return neighbours_of_type;
    }
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
    fn test_gas_cell() {
        let cell = Cell { mass: 1 };
        assert_eq!(cell.is_gas(), true);
    }
    #[test]
    fn test_rock_cell() {
        let cell = Cell { mass: 9999 };
        assert_eq!(cell.is_rock(), true);
    }
    #[test]
    fn test_star_cell() {
        let cell = Cell { mass: 59999 };
        assert_eq!(cell.is_star(), true);
    }

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

    #[test]
    fn test_neighbor_size_differs_for_different_types() {
        let mut universe = Universe::new(100);
        // gas
        universe.cells[0 as usize] = Cell { mass: 1 };
        let gas_neighbours = universe.neighbours_of_my_type(0).len();
        // rock
        universe.cells[0 as usize] = Cell { mass: 9999 };
        let rock_neighbours = universe.neighbours_of_my_type(0).len();
        // star
        universe.cells[0 as usize] = Cell { mass: 59999 };
        let star_neighbours = universe.neighbours_of_my_type(0).len();
        assert_ne!(gas_neighbours, rock_neighbours);
        assert_ne!(gas_neighbours, star_neighbours);
        assert_ne!(rock_neighbours, star_neighbours);
    }

    #[test]
    fn test_neighbor_size_same_for_small_universe() {
        let mut universe = Universe::new(3);
        // gas
        universe.cells[0 as usize] = Cell { mass: 1 };
        let gas_neighbours = universe.neighbours_of_my_type(0).len();
        // rock
        universe.cells[0 as usize] = Cell { mass: 9999 };
        let rock_neighbours = universe.neighbours_of_my_type(0).len();
        assert_eq!(gas_neighbours, rock_neighbours);
    }

}
