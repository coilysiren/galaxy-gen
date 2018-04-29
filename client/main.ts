import * as IWasmBinds from "./assets/built-wasm/galaxy_gen";

/**
 * MainScript is constructed with already compiled wasm
 * and does business logic with that wasm
 */
export class MainScript {
  private wasmBinds: typeof IWasmBinds;
  private universe: IWasmBinds.Universe;

  constructor(wasmBinds: typeof IWasmBinds) {
    this.wasmBinds = wasmBinds;
    this.start();
  }

  public start(): void {
    this.universe = this.wasmBinds.Universe.new(10);
    console.log(this.universe.cells());
  }
}
