/* tslint:disable */
import * as wasm from './galaxy_gen_backend_bg';

/**
*/
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
    /**
    * @param {number} arg0
    * @returns {Galaxy}
    */
    static new(arg0) {
        return Galaxy.__construct(wasm.galaxy_new(arg0));
    }
    /**
    * @returns {number}
    */
    cells_pointer() {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.galaxy_cells_pointer(this.ptr);
    }
    /**
    * @returns {void}
    */
    seed() {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.galaxy_seed(this.ptr);
    }
    /**
    * @returns {void}
    */
    tick() {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.galaxy_tick(this.ptr);
    }
}
/**
*/
export class Cell {
    
    static __construct(ptr) {
        return new Cell(ptr);
    }
    
    constructor(ptr) {
        this.ptr = ptr;
    }
    
    /**
    * @returns {number}
    */
    get mass() {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.__wbg_get_cell_mass(this.ptr);
    }
    set mass(arg0) {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.__wbg_set_cell_mass(this.ptr, arg0);
    }
    /**
    * @returns {number}
    */
    get accel_mangitude() {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.__wbg_get_cell_accel_mangitude(this.ptr);
    }
    set accel_mangitude(arg0) {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.__wbg_set_cell_accel_mangitude(this.ptr, arg0);
    }
    /**
    * @returns {number}
    */
    get accel_degree() {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.__wbg_get_cell_accel_degree(this.ptr);
    }
    set accel_degree(arg0) {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.__wbg_set_cell_accel_degree(this.ptr, arg0);
    }
    free() {
        const ptr = this.ptr;
        this.ptr = 0;
        wasm.__wbg_cell_free(ptr);
    }
    /**
    * @returns {number}
    */
    get_type() {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return wasm.cell_get_type(this.ptr);
    }
    /**
    * @returns {boolean}
    */
    is_gas() {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return (wasm.cell_is_gas(this.ptr)) !== 0;
    }
    /**
    * @returns {boolean}
    */
    is_star() {
        if (this.ptr === 0) {
            throw new Error('Attempt to use a moved value');
        }
        return (wasm.cell_is_star(this.ptr)) !== 0;
    }
}

let cachedDecoder = new TextDecoder('utf-8');

let cachegetUint8Memory = null;
function getUint8Memory() {
    if (cachegetUint8Memory === null || cachegetUint8Memory.buffer !== wasm.memory.buffer) {
        cachegetUint8Memory = new Uint8Array(wasm.memory.buffer);
    }
    return cachegetUint8Memory;
}

function getStringFromWasm(ptr, len) {
    return cachedDecoder.decode(getUint8Memory().subarray(ptr, ptr + len));
}

export function __wbindgen_throw(ptr, len) {
    throw new Error(getStringFromWasm(ptr, len));
}

