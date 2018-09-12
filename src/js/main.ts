import * as IWasmJSApi from "galaxy_gen_backend/galaxy_gen_backend";

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
  private galaxy: IWasmJSApi.Galaxy;
  private galaxySize: number;

  constructor(wasmJSApi: typeof IWasmJSApi, wasmBinary: IWasmBinary) {
    this.wasmJSApi = wasmJSApi;
    this.wasmBinary = wasmBinary;
  }

  public cells(): Uint8Array {
    return new Uint8Array(
      this.wasmBinary.memory.buffer,
      this.galaxy.cells_pointer(),
      this.memoryRange
    );
  }

  public generateData(size: number): void {
    this.galaxySize = size;
    this.galaxy = new this.wasmJSApi.Galaxy(size);
    this.galaxy.seed();
  }

  private get memoryRange(): number {
    return this.galaxySize ** 2;
  }
}
