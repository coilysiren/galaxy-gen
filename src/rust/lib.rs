#[macro_use]
extern crate specs_derive;

extern crate rand;
extern crate specs;
extern crate wasm_bindgen;

pub mod cell;
pub mod galaxy;
pub mod galaxyecs;
mod utils;

use cell::Cell;
use galaxy::Galaxy;

// public constants
impl Cell {
    pub const MIN_MASS_STAR: u16 = 10000;
}
impl Galaxy {
    pub const GAS_REACH_MODIFIER: u16 = 10;
}
