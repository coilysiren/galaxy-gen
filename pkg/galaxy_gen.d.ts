/* tslint:disable */
export class Galaxy {
free(): void;
static  new(arg0: number): Galaxy;

 cells_pointer(): number;

 seed(): void;

 tick(): void;

}
export class Cell {
mass: number
accel_mangitude: number
accel_degree: number
free(): void;
 get_type(): number;

 is_gas(): boolean;

 is_star(): boolean;

}
