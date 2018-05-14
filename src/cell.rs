#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

use wasm_bindgen::prelude::*;

// internal constants
impl Cell {
  pub const TYPE_INDEX_GAS: u8 = 0;
  pub const TYPE_INDEX_STAR: u8 = 2;
}

// types
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
  pub mass: u16,
  pub accel_mangitude: u16,
  pub accel_degree: u16,
}

// defaults
impl Default for Cell {
  fn default() -> Cell {
    Cell {
      mass: 0,
      accel_mangitude: 0,
      accel_degree: 0,
    }
  }
}

// public methods
#[wasm_bindgen]
impl Cell {
  pub fn get_type(&self) -> u8 {
    if self.mass < Cell::MIN_MASS_STAR {
      return Cell::TYPE_INDEX_GAS;
    } else {
      return Cell::TYPE_INDEX_STAR;
    }
  }
  pub fn is_gas(&self) -> bool {
    return self.check_if_type(Cell::TYPE_INDEX_GAS);
  }
  pub fn is_star(&self) -> bool {
    return self.check_if_type(Cell::TYPE_INDEX_STAR);
  }
}

// internal methods
impl Cell {
  pub fn check_if_type(&self, type_index: u8) -> bool {
    if (type_index == Cell::TYPE_INDEX_GAS) & (self.mass < Cell::MIN_MASS_STAR) {
      return true;
    } else if (type_index == Cell::TYPE_INDEX_STAR) & (self.mass >= Cell::MIN_MASS_STAR) {
      return true;
    } else {
      return false;
    }
  }
}
