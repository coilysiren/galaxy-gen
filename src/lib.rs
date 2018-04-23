#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

use wasm_bindgen::prelude::*;

struct Universe {
    width: u8,
    height: u8,
    cells: Vec<bool>,
}

impl Universe {
}

#[wasm_bindgen]
pub fn inform_logger(name: &str) -> String {
    let _boolean: bool = true;
    return format!("cat {} scratched the post", name)
}
