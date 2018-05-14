#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

use cell::*;
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
  pub fn new(size: u16) -> Galaxy {
    return Galaxy {
      size,
      cells: vec![
        Cell {
          mass: 0,
          ..Default::default()
        };
        (size as u32).pow(2) as usize
      ],
    };
  }
  pub fn cells_pointer(&self) -> *const Cell {
    return self.cells.as_ptr();
  }
  pub fn seed(&mut self) {
    for cell_index in 0..self.size.pow(2) {
      if cell_index % 3 == 0 {
        self.cells[cell_index as usize] = Cell {
          mass: 1,
          ..Default::default()
        };
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
