#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

mod cell;
mod galaxy;

// test cases
#[allow(dead_code)]
impl Galaxy {
  fn new_stable_case_one() -> Galaxy {
    let size = (3 as u16).pow(2);
    let mut galaxy = Galaxy {
      size,
      cells: vec![
        Cell {
          mass: 1,
          ..Default::default()
        };
        size.pow(2) as usize
      ],
    };
    galaxy.cells[4 as usize] = Cell {
      mass: 10,
      ..Default::default()
    };
    return galaxy;
  }
}

#[cfg(test)]
mod tests_cell_types {
  use super::*;
  #[test]
  fn test_gas_cell() {
    let cell = Cell {
      mass: 1,
      ..Default::default()
    };
    assert_eq!(cell.is_gas(), true);
  }
  #[test]
  fn test_star_cell() {
    let cell = Cell {
      mass: 59999,
      ..Default::default()
    };
    assert_eq!(cell.is_star(), true);
  }
}

#[cfg(test)]
mod tests_intial_generation {
  use super::*;
  #[test]
  fn test_inital_generation_no_panic() {
    Galaxy::new(10);
  }
  #[test]
  fn test_seed_no_panic() {
    Galaxy::new(10).seed();
  }
  #[test]
  fn test_seed_tick_no_panic() {
    let mut galaxy = Galaxy::new(10);
    galaxy.seed();
    galaxy.tick();
  }
  #[test]
  fn test_seed_alters_data() {
    let mut galaxy = Galaxy::new(10);
    let cells_before = galaxy.cells.clone();
    galaxy.seed();
    let cells_after = galaxy.cells.clone();
    assert_ne!(cells_before, cells_after);
  }
  #[test]
  fn test_stable_case_one_no_panics() {
    Galaxy::new_stable_case_one();
  }
}

#[cfg(test)]
mod tests_indexing {
  use super::*;
  #[test]
  fn test_index_to_row_col_start() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.index_to_row_col(0), (0, 0));
  }
  #[test]
  fn test_row_col_to_index_start() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.row_col_to_index(0, 0), 0);
  }
  #[test]
  fn test_index_to_row_col_center() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.index_to_row_col(4), (1, 1));
  }
  #[test]
  fn test_row_col_to_index_center() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.row_col_to_index(1, 1), 4);
  }
  #[test]
  fn test_index_to_row_col_end() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.index_to_row_col(8), (2, 2));
  }
  #[test]
  fn test_row_col_to_index_end() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.row_col_to_index(2, 2), 8);
  }
  #[test]
  fn test_index_edge_transform_top_right() {
    let galaxy = Galaxy::new(3);
    let index = 2;
    let (start, end) = galaxy.index_to_row_col(index);
    assert_eq!(galaxy.row_col_to_index(start, end), index);
  }
  #[test]
  fn test_index_edge_transform_bottom_left() {
    let galaxy = Galaxy::new(3);
    let index = 6;
    let (start, end) = galaxy.index_to_row_col(index);
    assert_eq!(galaxy.row_col_to_index(start, end), index);
  }
}

#[cfg(test)]
mod tests_neighbors_and_reach {
  use super::*;
  #[test]
  fn test_reach_range_start_edge() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.reach_range_start(0, 99), 0);
  }
  #[test]
  fn test_reach_range_start_overflow() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.reach_range_start(1, 99), 0);
  }
  #[test]
  fn test_reach_range_start_contained() {
    let galaxy = Galaxy::new(10);
    assert_eq!(galaxy.reach_range_start(4, 2), 2);
  }
  #[test]
  fn test_reach_range_end_edge() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.reach_range_end(2, 99), 2);
  }
  #[test]
  fn test_reach_range_end_overflow() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.reach_range_end(0, 99), 2);
  }
  #[test]
  fn test_reach_range_end_contained() {
    let galaxy = Galaxy::new(10);
    assert_eq!(galaxy.reach_range_end(2, 2), 4);
  }
  #[test]
  fn test_neighbor_size() {
    let galaxy = Galaxy::new(10);
    assert_eq!(galaxy.neighbours(0, 1).len(), 3);
  }
  #[test]
  fn test_neighbor_size_larger() {
    let galaxy = Galaxy::new(10);
    assert_eq!(galaxy.neighbours(0, 2).len(), 8);
  }
  #[test]
  fn test_neighbor_size_center() {
    let galaxy = Galaxy::new(3);
    assert_eq!(galaxy.neighbours(4, 1).len(), 8);
  }
  #[test]
  fn test_neighbor_size_differs_for_large_galaxy() {
    let mut galaxy = Galaxy::new(100);
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
    let mut galaxy = Galaxy::new(1);
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
