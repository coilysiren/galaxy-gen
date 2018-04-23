import * as Rust from "./assets/built-wasm/galaxy_gen";

export class MainScript {
  constructor(rustPromise: Promise<typeof Rust>) {
    rustPromise.then((rust: typeof Rust) => {
      console.log(rust.inform_logger("Rust and WebAssembly?"));
    });
  }
}
