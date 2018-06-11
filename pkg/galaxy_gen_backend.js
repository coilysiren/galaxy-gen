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

let slab = [];

let slab_next = 0;

function addHeapObject(obj) {
    if (slab_next === slab.length)
        slab.push(slab.length + 1);
    const idx = slab_next;
    const next = slab[idx];

    slab_next = next;

    slab[idx] = { obj, cnt: 1 };
    return idx << 1;
}

let stack = [];

function getObject(idx) {
    if ((idx & 1) === 1) {
        return stack[idx >> 1];
    } else {
        const val = slab[idx >> 1];

    return val.obj;

    }
}

export function __wbindgen_object_clone_ref(idx) {
    // If this object is on the stack promote it to the heap.
    if ((idx & 1) === 1)
        return addHeapObject(getObject(idx));

    // Otherwise if the object is on the heap just bump the
    // refcount and move on
    const val = slab[idx >> 1];
    val.cnt += 1;
    return idx;
}

function dropRef(idx) {

    let obj = slab[idx >> 1];

    obj.cnt -= 1;
    if (obj.cnt > 0)
        return;

    // If we hit 0 then free up our space in the slab
    slab[idx >> 1] = slab_next;
    slab_next = idx >> 1;
}

export function __wbindgen_object_drop_ref(i) { dropRef(i); }

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

export function __wbindgen_string_new(p, l) {
    return addHeapObject(getStringFromWasm(p, l));
}

export function __wbindgen_number_new(i) { return addHeapObject(i); }

export function __wbindgen_number_get(n, invalid) {
    let obj = getObject(n);
    if (typeof(obj) === 'number')
        return obj;
    getUint8Memory()[invalid] = 1;
    return 0;
}

export function __wbindgen_undefined_new() { return addHeapObject(undefined); }

export function __wbindgen_null_new() {
    return addHeapObject(null);
}

export function __wbindgen_is_null(idx) {
    return getObject(idx) === null ? 1 : 0;
}

export function __wbindgen_is_undefined(idx) {
    return getObject(idx) === undefined ? 1 : 0;
}

export function __wbindgen_boolean_new(v) {
    return addHeapObject(v === 1);
}

export function __wbindgen_boolean_get(i) {
    let v = getObject(i);
    if (typeof(v) === 'boolean') {
        return v ? 1 : 0;
    } else {
        return 2;
    }
}

export function __wbindgen_symbol_new(ptr, len) {
    let a;
    if (ptr === 0) {
        a = Symbol();
    } else {
        a = Symbol(getStringFromWasm(ptr, len));
    }
    return addHeapObject(a);
}

export function __wbindgen_is_symbol(i) {
    return typeof(getObject(i)) === 'symbol' ? 1 : 0;
}

let cachedEncoder = new TextEncoder('utf-8');

function passStringToWasm(arg) {

    const buf = cachedEncoder.encode(arg);
    const ptr = wasm.__wbindgen_malloc(buf.length);
    getUint8Memory().set(buf, ptr);
    return [ptr, buf.length];
}

let cachegetUint32Memory = null;
function getUint32Memory() {
    if (cachegetUint32Memory === null ||
        cachegetUint32Memory.buffer !== wasm.memory.buffer)
        cachegetUint32Memory = new Uint32Array(wasm.memory.buffer);
    return cachegetUint32Memory;
}

export function __wbindgen_string_get(i, len_ptr) {
    let obj = getObject(i);
    if (typeof(obj) !== 'string')
        return 0;
    const [ptr, len] = passStringToWasm(obj);
    getUint32Memory()[len_ptr / 4] = len;
    return ptr;
}

export function __wbindgen_throw(ptr, len) {
    throw new Error(getStringFromWasm(ptr, len));
}

