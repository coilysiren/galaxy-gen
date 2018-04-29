import * as IWasmBinds from "./assets/built-wasm/galaxy_gen";

/**
 * MainScript is constructed with already compiled wasm
 * and does business logic with that wasm
 */
export class MainScript {
  private wasmBinds: typeof IWasmBinds;

  constructor(wasmBinds: typeof IWasmBinds) {
    this.wasmBinds = wasmBinds;
    console.log(this.show_universe(10));
  }

  public show_universe(size: number): Uint8Array {
    return this.wasmBinds.show_universe(10);
  }
}
