#![feature(extern_prelude)]

#[macro_use]
extern crate cfg_if;
extern crate rand;
extern crate specs;
extern crate wasm_bindgen;

pub mod cell;
pub mod galaxy;
mod utils;

use cell::*;
use galaxy::*;

// public constants
impl Cell {
    pub const MIN_MASS_STAR: u16 = 10000;
}
impl Galaxy {
    pub const GAS_REACH_MODIFIER: u16 = 10;
}
