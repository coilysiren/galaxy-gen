/**
 * WebGPU compute-shader backend for N-body force calculation.
 *
 * Scope (MVP, matches README idea #9):
 *   - O(N²) direct-sum WGSL compute kernel.
 *   - Reads `positions` + `masses` storage buffers, writes `acc`.
 *   - JS ships accelerations back to the WASM `tick_with_accel` path,
 *     which runs the CPU integrator + collision step.
 *
 * This is *not* meant to beat the Barnes-Hut CPU path on big grids — the
 * all-pairs kernel is O(N²) and the readback round-trip dominates for
 * small N. The goal is to exercise the GPU code path end-to-end and give
 * us something to iterate on (bigger N, more complex kernels) later.
 *
 * Feature detection + graceful fallback live in `isWebGPUAvailable()`
 * and `WebGPUForceBackend.create()` — both return a clean result that
 * the Frontend can use to decide CPU vs GPU at runtime.
 */

export const WGSL_NBODY_FORCE = /* wgsl */ `
// Per-body input: packed as (x, y, mass, _pad) for 16-byte alignment.
struct Body {
  pos: vec2<f32>,
  mass: f32,
  _pad: f32,
};

struct Params {
  n: u32,
  g: f32,
  soft_sq: f32,
  _pad: f32,
};

@group(0) @binding(0) var<storage, read> bodies : array<Body>;
@group(0) @binding(1) var<storage, read_write> acc : array<vec2<f32>>;
@group(0) @binding(2) var<uniform> params : Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid : vec3<u32>) {
  let i = gid.x;
  if (i >= params.n) { return; }

  let me = bodies[i];
  var ax: f32 = 0.0;
  var ay: f32 = 0.0;

  // Skip zero-mass cells — they have no inertia in the integrator
  // anyway, and leaving them at zero acc matches the CPU path.
  if (me.mass == 0.0) {
    acc[i] = vec2<f32>(0.0, 0.0);
    return;
  }

  for (var j: u32 = 0u; j < params.n; j = j + 1u) {
    if (j == i) { continue; }
    let other = bodies[j];
    if (other.mass == 0.0) { continue; }
    let dx = other.pos.x - me.pos.x;
    let dy = other.pos.y - me.pos.y;
    let r2 = dx * dx + dy * dy + params.soft_sq;
    let inv_r = inverseSqrt(r2);
    let inv_r3 = inv_r * inv_r * inv_r;
    let k = params.g * inv_r3 * other.mass;
    ax = ax + k * dx;
    ay = ay + k * dy;
  }

  acc[i] = vec2<f32>(ax, ay);
}
`;

// Gravitational constants must match the Rust side (see
// `Galaxy::GRAVATIONAL_CONSTANT` / `SOFTENING_SQ` in galaxy.rs).
const G: number = 5.0e-2;
const SOFTENING_SQ: number = 1.0;

export function isWebGPUAvailable(): boolean {
  if (typeof navigator === "undefined") return false;
  return Boolean((navigator as any).gpu);
}

export interface WebGPUInitResult {
  ok: boolean;
  reason?: string;
  backend?: WebGPUForceBackend;
}

/**
 * Lazily initialized per-Frontend instance. Owns the GPUDevice,
 * pipeline, and the resizable storage buffers.
 *
 * The kernel runs per-tick: update bodies buffer → dispatch →
 * mapAsync the readback → copy into the `accel` Float32Array pair.
 */
export class WebGPUForceBackend {
  private device: GPUDevice;
  private pipeline: GPUComputePipeline;
  private n: number = 0;

  // Storage buffers — sized on first use / resize.
  private bodiesBuf: GPUBuffer | null = null;
  private accBuf: GPUBuffer | null = null;
  private readBuf: GPUBuffer | null = null;
  private paramsBuf: GPUBuffer;
  private bindGroup: GPUBindGroup | null = null;

  // Reusable typed-array scratch — avoids per-tick allocations.
  private bodiesScratch: Float32Array = new Float32Array(0);
  private accScratch: Float32Array = new Float32Array(0);
  private accX: Float32Array = new Float32Array(0);
  private accY: Float32Array = new Float32Array(0);

  private constructor(device: GPUDevice, pipeline: GPUComputePipeline) {
    this.device = device;
    this.pipeline = pipeline;
    this.paramsBuf = device.createBuffer({
      size: 16, // 4 × f32 = 16 bytes, already aligned for std140
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
  }

  /**
   * Try to spin up a WebGPU backend. Returns `{ ok: false }` if the
   * browser lacks WebGPU, if adapter/device request fails, or if
   * pipeline creation errors out — callers should fall back to CPU.
   */
  static async create(): Promise<WebGPUInitResult> {
    if (!isWebGPUAvailable()) {
      return { ok: false, reason: "navigator.gpu unavailable" };
    }
    try {
      const gpu = (navigator as any).gpu as GPU;
      const adapter = await gpu.requestAdapter();
      if (!adapter) {
        return { ok: false, reason: "no GPU adapter" };
      }
      const device = await adapter.requestDevice();
      device.lost.then((info) => {
        console.warn("[webgpu] device lost:", info.message);
      });

      const module = device.createShaderModule({ code: WGSL_NBODY_FORCE });
      const pipeline = await device.createComputePipelineAsync({
        layout: "auto",
        compute: { module, entryPoint: "main" },
      });
      return { ok: true, backend: new WebGPUForceBackend(device, pipeline) };
    } catch (err) {
      return { ok: false, reason: `webgpu init failed: ${String(err)}` };
    }
  }

  /** Grow / (re)allocate buffers and typed-array scratch for `n` bodies. */
  private ensureCapacity(n: number) {
    if (n === this.n && this.bodiesBuf && this.accBuf && this.readBuf) {
      return;
    }
    this.n = n;

    // Body stride = 16 bytes (vec2<f32> + f32 mass + f32 pad).
    const bodiesBytes = Math.max(16, n * 16);
    // vec2<f32> per body = 8 bytes, but WGSL pads vec2 in arrays to 8
    // so no extra pad needed here.
    const accBytes = Math.max(8, n * 8);

    if (this.bodiesBuf) this.bodiesBuf.destroy();
    if (this.accBuf) this.accBuf.destroy();
    if (this.readBuf) this.readBuf.destroy();

    this.bodiesBuf = this.device.createBuffer({
      size: bodiesBytes,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });
    this.accBuf = this.device.createBuffer({
      size: accBytes,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST,
    });
    this.readBuf = this.device.createBuffer({
      size: accBytes,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });

    this.bindGroup = this.device.createBindGroup({
      layout: this.pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.bodiesBuf } },
        { binding: 1, resource: { buffer: this.accBuf } },
        { binding: 2, resource: { buffer: this.paramsBuf } },
      ],
    });

    this.bodiesScratch = new Float32Array(Math.max(4, n * 4));
    this.accScratch = new Float32Array(Math.max(2, n * 2));
    this.accX = new Float32Array(n);
    this.accY = new Float32Array(n);
  }

  /**
   * Compute accelerations for every cell. Returns { acc_x, acc_y } as
   * parallel Float32Arrays matching the Rust `Galaxy` SoA layout.
   *
   * `mass` — Uint16Array of length n (n = size*size).
   * `size` — grid side length; positions are derived (col, row).
   */
  async computeAccelerations(
    mass: Uint16Array,
    size: number
  ): Promise<{ acc_x: Float32Array; acc_y: Float32Array }> {
    const n = mass.length;
    this.ensureCapacity(n);

    // Pack bodies as (x, y, mass, 0).
    const s = this.bodiesScratch;
    for (let i = 0; i < n; i++) {
      const off = i * 4;
      s[off + 0] = i % size;
      s[off + 1] = (i / size) | 0;
      s[off + 2] = mass[i];
      s[off + 3] = 0;
    }

    // Upload bodies + params.
    this.device.queue.writeBuffer(this.bodiesBuf!, 0, s.buffer, 0, n * 16);
    const params = new Float32Array(4);
    new Uint32Array(params.buffer)[0] = n;
    params[1] = G;
    params[2] = SOFTENING_SQ;
    params[3] = 0;
    this.device.queue.writeBuffer(this.paramsBuf, 0, params.buffer);

    // Dispatch.
    const encoder = this.device.createCommandEncoder();
    const pass = encoder.beginComputePass();
    pass.setPipeline(this.pipeline);
    pass.setBindGroup(0, this.bindGroup!);
    const workgroups = Math.ceil(n / 64);
    pass.dispatchWorkgroups(workgroups);
    pass.end();

    encoder.copyBufferToBuffer(this.accBuf!, 0, this.readBuf!, 0, n * 8);
    this.device.queue.submit([encoder.finish()]);

    // Map readback and split into parallel acc_x / acc_y arrays.
    await this.readBuf!.mapAsync(GPUMapMode.READ, 0, n * 8);
    const mapped = new Float32Array(this.readBuf!.getMappedRange(0, n * 8));
    // Copy out; we can't unmap while mapped arrays are referenced.
    this.accScratch.set(mapped.subarray(0, n * 2));
    this.readBuf!.unmap();

    for (let i = 0; i < n; i++) {
      this.accX[i] = this.accScratch[i * 2];
      this.accY[i] = this.accScratch[i * 2 + 1];
    }
    return { acc_x: this.accX, acc_y: this.accY };
  }

  destroy() {
    if (this.bodiesBuf) this.bodiesBuf.destroy();
    if (this.accBuf) this.accBuf.destroy();
    if (this.readBuf) this.readBuf.destroy();
    this.paramsBuf.destroy();
  }
}
