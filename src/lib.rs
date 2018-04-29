#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

use wasm_bindgen::prelude::*;

struct Universe {
    size: u8,
    cells: Vec<u8>,
}

impl Universe {
    pub fn new(size: u8) -> Universe {
        let cells = vec!(0; (size.pow(2)) as usize)
            .into_iter()
            .map(|i| {
                if i % 2 == 0 || i % 7 == 0 {
                    return 0
                } else {
                    return 1
                }
            })
            .collect();
        return Universe {
            size,
            cells,
        }
    }
}

#[wasm_bindgen]
pub fn show_universe(size: u8) -> Vec<u8> {
    return Universe::new(size).cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inital_generation() {
        show_universe(10);
    }
    #[test]
    #[should_panic]
    fn test_input_bounds() {
        show_universe(64);
    }
}
