#[macro_use]
extern crate specs_derive;

extern crate rand;
extern crate specs;
extern crate wasm_bindgen;

pub mod cell;
pub mod galaxy;
pub mod galaxyecs;

use galaxy::Galaxy;

// public constants
impl Galaxy {
    pub const MIN_MASS_STAR: u16 = 10000;
    pub const GAS_REACH_MODIFIER: u16 = 10;
}
