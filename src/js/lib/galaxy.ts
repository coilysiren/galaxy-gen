import * as wasm from "galaxy_gen_backend/galaxy_gen_backend";

export interface Cell {
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
  public galaxySize: number;

  constructor(galaxySize: number, minStarMass: number) {
    this.galaxy = new wasm.Galaxy(galaxySize, 0, minStarMass);
    this.galaxySize = galaxySize;
  }

  public seed(additionalMass: number): void {
    this.galaxy = this.galaxy.seed(additionalMass);
  }

  public tick(gravityReach: number): void {
    this.galaxy = this.galaxy.tick(gravityReach);
  }

  public cells(): Cell[] {
    const mass = this.galaxy.mass();
    const x = this.galaxy.x();
    const y = this.galaxy.y();
    const cells: Cell[] = [];
    for (let i = 0; i < this.galaxySize ** 2; i++) {
      cells.push({
        mass: mass[i],
        x: x[i],
        y: y[i],
      });
    }
    return cells;
  }
}
