use cell::*;
use rand::Rng;
use wasm_bindgen::prelude::*;

// types
#[wasm_bindgen]
pub struct Galaxy {
    size: u16,
    cells: Vec<Cell>,
}

// public methods
#[wasm_bindgen]
impl Galaxy {
    #[wasm_bindgen(constructor)]
    pub fn new(size: u16, mass: u16) -> Galaxy {
        // create a new galaxy
        // https://github.com/rustwasm/console_error_panic_hook#readme
        console_error_panic_hook::set_once();
        return Galaxy {
            size,
            cells: vec![
                Cell {
                    mass: mass,
                    ..Default::default()
                };
                (size as u32).pow(2) as usize
            ],
        };
    }
    pub fn seed(&self, additional: u16) -> Galaxy {
        // add mass to the galaxy
        let mut next = Vec::with_capacity((self.size as usize).pow(2));
        for cell_index in 0..self.size.pow(2) {
            let mass = self.cells[cell_index as usize].mass;
            next.push(Cell {
                mass: mass + rand::thread_rng().gen_range(0, additional + 1),
                ..Default::default()
            });
        }
        return Galaxy {
            size: self.size,
            cells: next,
        };
    }
    pub fn tick(&self, reach: u16) -> Galaxy {
        // advance the galaxy one tick
        // TODO: not yet implemented
        let next = Vec::with_capacity((self.size as usize).pow(2));
        for index in 0..(self.size - 1) {
            let _cell = self.cells[index as usize];
            let _neighbours = self.neighbours(index, reach);
        }
        return Galaxy {
            size: self.size,
            cells: next,
        };
    }
    pub fn mass(&self) -> Vec<u16> {
        // for every cell, get the mass
        let mut mass = Vec::new();
        for cell in self.cells.iter() {
            mass.push(cell.mass);
        }
        return mass;
    }
}

// private methods
impl Galaxy {
    fn reach_of_type(&self, type_index: u8) -> u16 {
        match type_index {
            Cell::TYPE_INDEX_GAS => self.size / Galaxy::GAS_REACH_MODIFIER + 1,
            Cell::TYPE_INDEX_STAR => self.size,
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
    fn col_row_to_index(&self, col: u16, row: u16) -> u16 {
        return row * self.size + col;
    }
    fn index_to_row_col(&self, index: u16) -> (u16, u16) {
        return (index % self.size, index / self.size);
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
mod tests_intial_generation {
    use super::*;
    #[test]
    fn test_inital_generation_no_panic() {
        Galaxy::new(10, 0);
    }
    #[test]
    fn test_seed_no_panic() {
        Galaxy::new(10, 0).seed(1);
    }
    #[test]
    fn test_seed_tick_no_panic() {
        Galaxy::new(10, 1).seed(1).tick(1);
    }
    #[test]
    fn test_seed_alters_data() {
        let mut galaxy = Galaxy::new(10, 0);
        let cells_before = galaxy.cells.clone();
        galaxy = galaxy.seed(1);
        let cells_after = galaxy.cells.clone();
        assert_ne!(cells_before, cells_after);
    }
    #[test]
    fn test_seed_doesnt_alter_when_zero() {
        let mut galaxy = Galaxy::new(10, 0);
        let cells_before = galaxy.cells.clone();
        galaxy = galaxy.seed(0);
        let cells_after = galaxy.cells.clone();
        assert_eq!(cells_before, cells_after);
    }
    #[test]
    fn test_seed_alters_data_twice() {
        let mut galaxy = Galaxy::new(10, 0);
        let cells_first = galaxy.cells.clone();

        galaxy = galaxy.seed(1);
        let cells_second = galaxy.cells.clone();
        assert_ne!(cells_first, cells_second);

        galaxy = galaxy.seed(1);
        let cells_third = galaxy.cells.clone();
        assert_ne!(cells_first, cells_second);
        assert_ne!(cells_first, cells_third);
        assert_ne!(cells_second, cells_third);
    }
}

#[cfg(test)]
mod tests_indexing {
    use super::*;
    #[test]
    fn test_index_to_row_col_start() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.index_to_row_col(0), (0, 0));
    }
    #[test]
    fn test_col_row_to_index_start() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.col_row_to_index(0, 0), 0);
    }
    #[test]
    fn test_index_to_row_col_center() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.index_to_row_col(4), (1, 1));
    }
    #[test]
    fn test_col_row_to_index_center() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.col_row_to_index(1, 1), 4);
    }
    #[test]
    fn test_index_to_row_col_end() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.index_to_row_col(8), (2, 2));
    }
    #[test]
    fn test_col_row_to_index_end() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.col_row_to_index(2, 2), 8);
    }
    #[test]
    fn index_to_row_col_first() {
        let galaxy = Galaxy::new(3, 0);
        let index = 1;
        let (x, y) = galaxy.index_to_row_col(index);
        assert_eq!(x, 1);
        assert_eq!(y, 0);
    }
    #[test]
    fn index_to_row_col_second() {
        let galaxy = Galaxy::new(3, 0);
        let index = 2;
        let (x, y) = galaxy.index_to_row_col(index);
        assert_eq!(x, 2);
        assert_eq!(y, 0);
    }
    #[test]
    fn test_index_edge_transform_top_right() {
        let galaxy = Galaxy::new(3, 0);
        let index = 2;
        let (x, y) = galaxy.index_to_row_col(index);
        assert_eq!(galaxy.col_row_to_index(x, y), index);
        assert_eq!(x, 2);
        assert_eq!(y, 0);
    }
    #[test]
    fn test_index_edge_transform_bottom_left() {
        let galaxy = Galaxy::new(3, 0);
        let index = 6;
        let (x, y) = galaxy.index_to_row_col(index);
        assert_eq!(galaxy.col_row_to_index(x, y), index);
        assert_eq!(x, 0);
        assert_eq!(y, 2);
    }
}

#[cfg(test)]
mod tests_neighbors_and_reach {
    use super::*;
    #[test]
    fn test_reach_range_start_edge() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.reach_range_start(0, 99), 0);
    }
    #[test]
    fn test_reach_range_start_overflow() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.reach_range_start(1, 99), 0);
    }
    #[test]
    fn test_reach_range_start_contained() {
        let galaxy = Galaxy::new(10, 0);
        assert_eq!(galaxy.reach_range_start(4, 2), 2);
    }
    #[test]
    fn test_reach_range_end_edge() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.reach_range_end(2, 99), 2);
    }
    #[test]
    fn test_reach_range_end_overflow() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.reach_range_end(0, 99), 2);
    }
    #[test]
    fn test_reach_range_end_contained() {
        let galaxy = Galaxy::new(10, 0);
        assert_eq!(galaxy.reach_range_end(2, 2), 4);
    }
    #[test]
    fn test_neighbor_size() {
        let galaxy = Galaxy::new(10, 0);
        assert_eq!(galaxy.neighbours(0, 1).len(), 3);
    }
    #[test]
    fn test_neighbor_size_larger() {
        let galaxy = Galaxy::new(10, 0);
        assert_eq!(galaxy.neighbours(0, 2).len(), 8);
    }
    #[test]
    fn test_neighbor_size_center() {
        let galaxy = Galaxy::new(3, 0);
        assert_eq!(galaxy.neighbours(4, 1).len(), 8);
    }
    #[test]
    fn test_neighbor_size_differs_for_large_galaxy() {
        let mut galaxy = Galaxy::new(100, 0);
        let index = 0 as usize;
        // gas
        galaxy.cells[index] = Cell {
            mass: 1,
            ..Default::default()
        };
        let gas_neighbours = galaxy.neighbours_of_my_type(0).len();
        // star
        galaxy.cells[index] = Cell {
            mass: 59999,
            ..Default::default()
        };
        let star_neighbours = galaxy.neighbours_of_my_type(0).len();
        assert_ne!(gas_neighbours, star_neighbours);
    }
    #[test]
    fn test_neighbor_size_same_for_small_galaxy() {
        let mut galaxy = Galaxy::new(1, 0);
        let index = 0 as usize;
        // gas
        galaxy.cells[index] = Cell {
            mass: 1,
            ..Default::default()
        };
        let gas_neighbours = galaxy.neighbours_of_my_type(0).len();
        // star
        galaxy.cells[index] = Cell {
            mass: 59999,
            ..Default::default()
        };
        let star_neighbours = galaxy.neighbours_of_my_type(0).len();
        assert_eq!(gas_neighbours, star_neighbours);
    }
}
