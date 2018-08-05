#![feature(use_extern_macros)]

extern crate wasm_bindgen;

pub mod cell;
pub mod galaxy;

use cell::*;
use galaxy::*;

// public constants
impl Cell {
    pub const MIN_MASS_STAR: u16 = 10000;
}
impl Galaxy {
    pub const GAS_REACH_MODIFIER: u16 = 10;
}
