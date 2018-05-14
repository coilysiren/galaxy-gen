#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

mod cell;
mod galaxy;

use cell::*;
use galaxy::*;

impl Cell {
    pub const MIN_MASS_STAR: u16 = 10000;
}

impl Galaxy {
    pub const GAS_REACH_MODIFIER: u16 = 10;
}
