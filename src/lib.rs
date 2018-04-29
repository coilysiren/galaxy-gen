#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

use wasm_bindgen::prelude::*;

struct Universe {
    width: u8,
    height: u8,
    cells: Vec<u8>,
}

impl Universe {
    pub fn new(height: u8, width: u8) -> Universe {
        let cells = (0..width * height)
            .map(|i| {
                if i % 2 == 0 || i % 7 == 0 {
                    return 0
                } else {
                    return 1
                }
            })
            .collect();

        return Universe {
            width,
            height,
            cells,
        }
    }
}

#[wasm_bindgen]
pub fn show_universe(height: u8, width: u8) -> Vec<u8> {
    return Universe::new(height, width).cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inital_generation() {
        show_universe(64, 64);
    }
    #[test]
    #[should_panic]
    fn test_input_bounds() {
        show_universe(1000, 1000);
    }
}
