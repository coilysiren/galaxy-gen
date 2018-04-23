import * as Rust from "./assets/built-wasm/galaxy_gen";

export class MainScript {
  constructor(rust: typeof Rust) {
    console.log(rust.inform_logger("Rust and WebAssembly?"));
  }
}
