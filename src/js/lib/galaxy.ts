import * as wasm from "galaxy_gen_backend/galaxy_gen_backend";

export interface Cell {
  mass: number;
  x: number;
  y: number;
}

/**
 * Thin JS wrapper over the Rust/WASM Galaxy. `massArray()` returns a
 * `Uint16Array` (a single memcpy from WASM linear memory, courtesy of
 * wasm-bindgen). Positions are derived from cell index, so only masses
 * cross the boundary each tick.
 */
export class Frontend {
  private galaxy: wasm.Galaxy;
  public galaxySize: number;

  constructor(galaxySize: number, minStarMass: number) {
    this.galaxy = new wasm.Galaxy(galaxySize, 0, minStarMass);
    this.galaxySize = galaxySize;
  }

  public seed(additionalMass: number): void {
    const next = this.galaxy.seed(additionalMass);
    this.galaxy.free();
    this.galaxy = next;
  }

  public tick(timeModifier: number): void {
    const next = this.galaxy.tick(timeModifier);
    this.galaxy.free();
    this.galaxy = next;
  }

  /** Fast path for the renderer — one memcpy, no per-cell object churn. */
  public massArray(): Uint16Array {
    return this.galaxy.mass();
  }

  /** Legacy API. Allocates a Cell[]; avoid on the hot path. */
  public cells(): Cell[] {
    const mass = this.massArray();
    const size = this.galaxySize;
    const out: Cell[] = new Array(mass.length);
    for (let i = 0; i < mass.length; i++) {
      out[i] = { mass: mass[i], x: i % size, y: (i / size) | 0 };
    }
    return out;
  }
}
