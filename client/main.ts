import * as IWasmJSApi from "./assets/built-wasm/galaxy_gen";

interface IWasmBinary {
  memory: WebAssembly.Memory;
}

/**
 * MainScript is constructed with already compiled wasm
 * and does business logic with that wasm
 */
export class MainScript {
  private wasmJSApi: typeof IWasmJSApi;
  private wasmBinary: IWasmBinary;
  private universe: IWasmJSApi.Universe;
  private universeSize: number;

  constructor(wasmJSApi: typeof IWasmJSApi, wasmBinary: IWasmBinary) {
    this.wasmJSApi = wasmJSApi;
    this.wasmBinary = wasmBinary;
    this.start(10);
  }

  private start(size: number): void {
    this.universeSize = size;
    this.universe = this.wasmJSApi.Universe.new(size);
    console.log(this.cells());
  }

  private cells(): Uint8Array {
    return new Uint8Array(
      this.wasmBinary.memory.buffer,
      this.universe.cells_pointer(),
      this.memoryRange,
    );
  }

  private get memoryRange(): number {
    return this.universeSize ** 2;
  }
}
