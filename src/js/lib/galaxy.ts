import * as wasm from "galaxy_gen_backend/galaxy_gen_backend";

interface Cell {
  mass: number;
  x: number;
  y: number;
}

/**
 * Main is constructed with already compiled wasm
 * and does business logic with that wasm
 */
export class Frontend {
  private galaxy: wasm.Galaxy; // pointer to galaxy
  private galaxySize: number;

  constructor(galaxySize: number) {
    this.galaxy = new wasm.Galaxy(galaxySize, 0);
    this.galaxySize = galaxySize;
  }

  public seed(): void {
    this.galaxy.seed();
  }

  public tick(): void {
    this.galaxy.tick();
  }

  public cells(): Cell[] {
    // Uint16Array to list of numbers
    let cells: Cell[] = [];
    const mass = Array.from(this.galaxy.cell_mass());
    mass.forEach((element, index) => {
      cells.push({
        mass: element,
        x: index % this.galaxySize,
        y: Math.floor(index / this.galaxySize),
      });
    });
    return cells;
  }
}
