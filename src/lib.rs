#![feature(proc_macro, wasm_custom_section, wasm_import_module)]

extern crate wasm_bindgen;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Universe {
    size: u8,
    cells: Vec<u8>,
}

#[wasm_bindgen]
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
    pub fn cells(&self) -> *const u8 {
        self.cells.as_ptr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inital_generation() {
        Universe::new(10);
    }
    #[test]
    #[should_panic]
    fn test_input_bounds() {
        Universe::new(64);
    }
}
