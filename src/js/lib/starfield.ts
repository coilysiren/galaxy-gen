/// Seeded deep-space backdrop: the black sky the galaxy hangs in.
///
/// Built once per (seed, canvas size) into an offscreen canvas and
/// blitted as the opaque base of every frame, so the per-frame cost is
/// one `drawImage` in place of the `fillRect` it replaces - a wash.
/// Baking the space-black base into this canvas is also what keeps the
/// galaxy's multiply-composited dust honest: it needs opaque pixels
/// underneath or it stamps grey squares.
///
/// It reuses the renderer's own sprites rather than inventing a second
/// visual language, so the backdrop reads as the same universe: the
/// four nebular gas tiers, the dust sprite, and the stellar-class
/// colors. Everything is pulled well down in alpha - this is scenery,
/// and it must never compete with the simulated galaxy in front of it.
///
/// The backdrop is screen-fixed. It is drawn before the camera and the
/// co-rotating frame rotation, so it does not spin with the galaxy -
/// distant sky does not share the disk's rotation.

export interface StarfieldAssets {
  /// Nebular gas sprites, tier-major, as built by the renderer.
  gasSprites: HTMLCanvasElement[][];
  dustSprite: HTMLCanvasElement | null;
  /// Stellar-class colors, cool -> hot, then giant / dwarf / compact.
  starColors: string[];
}

export interface StarfieldOptions {
  width: number;
  height: number;
  dpr: number;
  seed: bigint | null;
  assets: StarfieldAssets;
}

/// Global dimmer on everything the backdrop draws. The single knob to
/// turn if the sky ever starts pulling attention off the galaxy.
const FADE = 0.55;

/// Star count at the reference viewport, scaled by area from there.
const REFERENCE_AREA = 1280 * 720;
const STARS_PER_REFERENCE = 1150;
/// Upper bound so a huge viewport cannot run the count away. Resize
/// stability comes from normalized coords - see docs/starfield.md.
const STAR_COUNT_CAP = 2400;

/// Indices into the renderer's `GAS_TIERS`. The [OIII] teal tier (3)
/// is deliberately absent - see docs/starfield.md.
const NEBULA_TIER_WEIGHTS = [0, 0, 0, 1, 1, 2];

/// Haze fades inside this fraction of the short edge, leaving the disk
/// a clean dark field behind it.
const CENTER_KEEPOUT = 0.42;

const NEBULA_CLOUDS = 6;
const NEBULA_STAMPS = 140;
/// Well above the sprites' own alpha: the galaxy stacks dozens per
/// pixel, the backdrop spreads them thinly. See docs/starfield.md.
const NEBULA_ALPHA = 0.62;
const DUST_STAMPS = 14;

const SPACE_BLACK = "#05060a";

/// mulberry32, seeded from a fold of the u64 sim seed so a `?seed=`
/// permalink reproduces the sky along with the galaxy.
function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/// Fold a u64 into 32 bits, mixing both halves so seeds differing only
/// in their high word still diverge. The salt separates streams.
function foldSeed(seed: bigint | null, salt: number): number {
  if (seed == null) return salt >>> 0;
  const lo = Number(seed & 0xffffffffn) >>> 0;
  const hi = Number((seed >> 32n) & 0xffffffffn) >>> 0;
  return (Math.imul(lo ^ salt, 0x9e3779b1) ^ Math.imul(hi + salt, 0x85ebca6b)) >>> 0;
}

/// 0 at frame center, easing to 1 outside the keep-out radius. Pixel
/// space, not normalized, so it stays circular on a wide viewport.
function centerKeepout(nx: number, ny: number, width: number, height: number): number {
  const dx = nx * width - width / 2;
  const dy = ny * height - height / 2;
  const r = Math.sqrt(dx * dx + dy * dy) / (Math.min(width, height) * CENTER_KEEPOUT);
  if (r >= 1) return 1;
  return r * r * (3 - 2 * r);
}

function gaussian(rand: () => number): number {
  // Box-Muller. Clouds want a soft falloff, not a hard disc.
  const u = Math.max(1e-6, rand());
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * rand());
}

export function buildStarfield(opts: StarfieldOptions): HTMLCanvasElement {
  const { width, height, dpr, seed, assets } = opts;
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(width * dpr));
  canvas.height = Math.max(1, Math.round(height * dpr));
  const ctx = canvas.getContext("2d")!;
  ctx.scale(dpr, dpr);

  ctx.fillStyle = SPACE_BLACK;
  ctx.fillRect(0, 0, width, height);

  // Sprite sizes key off the short edge so clouds stay circular on a
  // wide viewport instead of stretching with the normalized coords.
  const shortEdge = Math.min(width, height);

  // Seeded great-circle band: the distant-galactic-plane cue stars and
  // haze concentrate along.
  const bandRand = rng(foldSeed(seed, 0x51ed));
  const bandAngle = bandRand() * Math.PI;
  const bandCos = Math.cos(bandAngle);
  const bandSin = Math.sin(bandAngle);
  const bandOffset = (bandRand() - 0.5) * 0.35;
  const bandWidth = 0.2 + bandRand() * 0.14;
  /// Perpendicular distance from the band, 0 on it, 1 far off it.
  const bandDistance = (nx: number, ny: number) => {
    const d = (nx - 0.5) * bandSin - (ny - 0.5) * bandCos - bandOffset;
    return Math.min(1, Math.abs(d) / bandWidth);
  };

  drawNebulae(ctx, width, height, shortEdge, seed, assets, bandDistance);
  drawStars(ctx, width, height, seed, assets, bandDistance);

  return canvas;
}

function drawNebulae(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  shortEdge: number,
  seed: bigint | null,
  assets: StarfieldAssets,
  bandDistance: (nx: number, ny: number) => number
) {
  const { gasSprites, dustSprite } = assets;
  if (gasSprites.length === 0) return;
  const rand = rng(foldSeed(seed, 0x9e3b));

  ctx.save();
  // Additive, like the galaxy's own gas passes: overlapping stamps
  // accumulate into brightness instead of flatly replacing.
  ctx.globalCompositeOperation = "lighter";

  for (let cloud = 0; cloud < NEBULA_CLOUDS; cloud++) {
    // Pull cloud centers toward the band so the haze reads as one
    // structure rather than scattered blobs.
    const bias = rand();
    const cx = rand();
    const cy = bias < 0.7 ? 0.5 + (rand() - 0.5) * 0.5 : rand();
    const tierIndex = NEBULA_TIER_WEIGHTS[Math.floor(rand() * NEBULA_TIER_WEIGHTS.length)];
    const tier = gasSprites[Math.min(gasSprites.length - 1, tierIndex)];
    const sprite = tier[Math.floor(rand() * tier.length)];
    const spread = (0.06 + rand() * 0.11) * shortEdge;
    // Cool tiers dominate; a hot cloud is an occasional accent.
    const alpha = NEBULA_ALPHA * FADE * (0.5 + rand() * 0.9);

    ctx.globalAlpha = alpha;
    for (let i = 0; i < NEBULA_STAMPS; i++) {
      const nx = cx + (gaussian(rand) * spread) / width;
      const ny = cy + (gaussian(rand) * spread) / height;
      if (nx < -0.1 || nx > 1.1 || ny < -0.1 || ny > 1.1) continue;
      // Thin away from the band so it is not uniform fog, and toward
      // the center so it stays off the galaxy.
      const falloff = (1 - 0.65 * bandDistance(nx, ny)) * centerKeepout(nx, ny, width, height);
      if (falloff <= 0.05) continue;
      const r = shortEdge * (0.03 + rand() * 0.075);
      ctx.globalAlpha = alpha * falloff;
      ctx.drawImage(sprite, nx * width - r, ny * height - r, r * 2, r * 2);
    }
  }

  // Multiplied so it absorbs rather than adds: it only bites where a
  // cloud has already brightened the pixels underneath.
  if (dustSprite) {
    ctx.globalCompositeOperation = "multiply";
    for (let i = 0; i < DUST_STAMPS; i++) {
      const nx = rand();
      const ny = rand();
      const falloff = (1 - bandDistance(nx, ny)) * centerKeepout(nx, ny, width, height);
      if (falloff <= 0.1) continue;
      ctx.globalAlpha = 0.16 * FADE * falloff;
      const r = shortEdge * (0.04 + rand() * 0.09);
      ctx.drawImage(dustSprite, nx * width - r, ny * height - r, r * 2, r * 2);
    }
  }

  ctx.restore();
}

function drawStars(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  seed: bigint | null,
  assets: StarfieldAssets,
  bandDistance: (nx: number, ny: number) => number
) {
  const { starColors } = assets;
  if (starColors.length === 0) return;
  const rand = rng(foldSeed(seed, 0x2f17));

  const wanted = Math.round((STARS_PER_REFERENCE * width * height) / REFERENCE_AREA);
  const count = Math.max(120, Math.min(STAR_COUNT_CAP, wanted));

  ctx.save();
  ctx.globalCompositeOperation = "lighter";

  for (let i = 0; i < count; i++) {
    const nx = rand();
    const ny = rand();
    // Band concentration: a star far off the band survives only on a
    // second roll, which thins the field without emptying it.
    const off = bandDistance(nx, ny);
    const keep = 1 - 0.55 * off;
    const brightRoll = rand();
    const classRoll = rand();
    const sizeRoll = rand();
    if (rand() > keep) continue;

    // Pushed toward the cool buckets; the giant/dwarf/compact entries
    // at the top of the table are reached only rarely.
    const classCount = starColors.length;
    const idx =
      classRoll < 0.94
        ? Math.floor(Math.pow(classRoll / 0.94, 1.7) * (classCount - 3))
        : classCount - 3 + Math.floor(rand() * 3);
    ctx.fillStyle = starColors[Math.min(classCount - 1, idx)];

    // Brightness is square-law-ish: a field of uniformly-lit points
    // looks like noise, not a sky.
    const bright = Math.pow(brightRoll, 2.2);
    const alpha = (0.12 + bright * 0.78) * FADE * (0.55 + 0.45 * (1 - off));
    const radius = 0.35 + Math.pow(sizeRoll, 3) * 1.5;

    ctx.globalAlpha = alpha;
    ctx.beginPath();
    ctx.arc(nx * width, ny * height, radius, 0, Math.PI * 2);
    ctx.fill();

    // The brightest few get a soft halo and a faint diffraction cross,
    // the cue that sells "bright star" rather than "big dot".
    if (bright > 0.82) {
      ctx.globalAlpha = alpha * 0.22;
      ctx.beginPath();
      ctx.arc(nx * width, ny * height, radius * 3.2, 0, Math.PI * 2);
      ctx.fill();

      ctx.globalAlpha = alpha * 0.3;
      const spike = radius * 4.5;
      ctx.fillRect(nx * width - spike, ny * height - 0.35, spike * 2, 0.7);
      ctx.fillRect(nx * width - 0.35, ny * height - spike, 0.7, spike * 2);
    }
  }

  ctx.restore();
}
