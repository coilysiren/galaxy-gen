import * as Rust from "./assets/built-wasm/galaxy_gen";

export class MainScript {
  constructor(rust: typeof Rust) {
    const universe: Uint8Array = rust.show_universe(64, 64);
  }
}
