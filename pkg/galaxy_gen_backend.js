/* tslint:disable */
import * as wasm from './galaxy_gen_backend_bg';

export class Cell {

                static __construct(ptr) {
                    return new Cell(ptr);
                }

                constructor(ptr) {
                    this.ptr = ptr;
                }
            get mass() {
    return wasm.__wbg_get_cell_mass(this.ptr);
}
set mass(arg0) {
    return wasm.__wbg_set_cell_mass(this.ptr, arg0);
}get accel_mangitude() {
    return wasm.__wbg_get_cell_accel_mangitude(this.ptr);
}
set accel_mangitude(arg0) {
    return wasm.__wbg_set_cell_accel_mangitude(this.ptr, arg0);
}get accel_degree() {
    return wasm.__wbg_get_cell_accel_degree(this.ptr);
}
set accel_degree(arg0) {
    return wasm.__wbg_set_cell_accel_degree(this.ptr, arg0);
}get mass() {
    return wasm.__wbg_get_cell_mass(this.ptr);
}
set mass(arg0) {
    return wasm.__wbg_set_cell_mass(this.ptr, arg0);
}get accel_mangitude() {
    return wasm.__wbg_get_cell_accel_mangitude(this.ptr);
}
set accel_mangitude(arg0) {
    return wasm.__wbg_set_cell_accel_mangitude(this.ptr, arg0);
}get accel_degree() {
    return wasm.__wbg_get_cell_accel_degree(this.ptr);
}
set accel_degree(arg0) {
    return wasm.__wbg_set_cell_accel_degree(this.ptr, arg0);
}get mass() {
    return wasm.__wbg_get_cell_mass(this.ptr);
}
set mass(arg0) {
    return wasm.__wbg_set_cell_mass(this.ptr, arg0);
}get accel_mangitude() {
    return wasm.__wbg_get_cell_accel_mangitude(this.ptr);
}
set accel_mangitude(arg0) {
    return wasm.__wbg_set_cell_accel_mangitude(this.ptr, arg0);
}get accel_degree() {
    return wasm.__wbg_get_cell_accel_degree(this.ptr);
}
set accel_degree(arg0) {
    return wasm.__wbg_set_cell_accel_degree(this.ptr, arg0);
}
            free() {
                const ptr = this.ptr;
                this.ptr = 0;
                wasm.__wbg_cell_free(ptr);
            }
        get_type() {
    return wasm.cell_get_type(this.ptr);
}
is_gas() {
    return (wasm.cell_is_gas(this.ptr)) !== 0;
}
is_star() {
    return (wasm.cell_is_star(this.ptr)) !== 0;
}
get_type() {
    return wasm.cell_get_type(this.ptr);
}
is_gas() {
    return (wasm.cell_is_gas(this.ptr)) !== 0;
}
is_star() {
    return (wasm.cell_is_star(this.ptr)) !== 0;
}
get_type() {
    return wasm.cell_get_type(this.ptr);
}
is_gas() {
    return (wasm.cell_is_gas(this.ptr)) !== 0;
}
is_star() {
    return (wasm.cell_is_star(this.ptr)) !== 0;
}
}

export class Galaxy {

                static __construct(ptr) {
                    return new Galaxy(ptr);
                }

                constructor(ptr) {
                    this.ptr = ptr;
                }

            free() {
                const ptr = this.ptr;
                this.ptr = 0;
                wasm.__wbg_galaxy_free(ptr);
            }
        static new(arg0) {
    return Galaxy.__construct(wasm.galaxy_new(arg0));
}
cells_pointer() {
    return wasm.galaxy_cells_pointer(this.ptr);
}
seed() {
    return wasm.galaxy_seed(this.ptr);
}
tick() {
    return wasm.galaxy_tick(this.ptr);
}
}

let cachedDecoder = new TextDecoder('utf-8');

let cachegetUint8Memory = null;
function getUint8Memory() {
    if (cachegetUint8Memory === null ||
        cachegetUint8Memory.buffer !== wasm.memory.buffer)
        cachegetUint8Memory = new Uint8Array(wasm.memory.buffer);
    return cachegetUint8Memory;
}

function getStringFromWasm(ptr, len) {
    return cachedDecoder.decode(getUint8Memory().subarray(ptr, ptr + len));
}

export function __wbindgen_throw(ptr, len) {
    throw new Error(getStringFromWasm(ptr, len));
}

