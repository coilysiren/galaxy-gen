import * as Rust from "./assets/built-wasm/galaxy_gen";

export class MainScript {
  private rust: typeof Rust;

  constructor(rust: typeof Rust) {
    this.rust = rust;
    console.log(this.show_universe(10));
  }

  public show_universe(size: number): Uint8Array {
    return this.rust.show_universe(10);
  }
}
