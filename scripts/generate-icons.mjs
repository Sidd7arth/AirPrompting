#!/usr/bin/env node
/**
 * Generates AirPrompt's icon source files.
 *
 * Writes real PNG bytes from scratch (zlib + a CRC32 implementation) so the
 * repository carries no binary blobs that cannot be reviewed or regenerated.
 * `npx tauri icon` then derives every platform size, .icns and .ico from
 * `source.png`.
 *
 * The mark is an original five-bar voice waveform on a rounded square. It
 * deliberately contains nothing resembling any existing product's artwork.
 *
 *   node scripts/generate-icons.mjs
 *   npx tauri icon src-tauri/icons/source.png -o src-tauri/icons
 */
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const ICONS = join(ROOT, "src-tauri", "icons");

/* ------------------------------------------------------------------ colours */

const ACCENT = [31, 181, 140]; // teal — survives both light and dark surfaces
const BACKDROP = [20, 22, 27]; // near-black, matches the app's dark surface

/* --------------------------------------------------------------- PNG writer */

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buffer) {
  let crc = -1;
  for (let i = 0; i < buffer.length; i++) {
    crc = (crc >>> 8) ^ CRC_TABLE[(crc ^ buffer[i]) & 0xff];
  }
  return (crc ^ -1) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const typed = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(typed), 0);
  return Buffer.concat([length, typed, crc]);
}

/** Encode RGBA pixels (Buffer, width*height*4) as an 8-bit RGBA PNG. */
function encodePng(width, height, rgba) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8; // bit depth
  header[9] = 6; // colour type: truecolour with alpha
  header[10] = 0; // deflate
  header[11] = 0; // adaptive filtering
  header[12] = 0; // no interlace

  // One filter byte (0 = None) per scanline.
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0;
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0))
  ]);
}

/* --------------------------------------------------------------- geometry */

/** True when (px, py) lies inside a rounded rectangle. */
function inRoundedRect(px, py, cx, cy, w, h, radius) {
  const r = Math.min(radius, w / 2, h / 2);
  const dx = Math.max(Math.abs(px - cx) - (w / 2 - r), 0);
  const dy = Math.max(Math.abs(py - cy) - (h / 2 - r), 0);
  return dx * dx + dy * dy <= r * r;
}

/**
 * The waveform mark: five pill-shaped bars with a symmetric height profile.
 * Coordinates are expressed as a fraction of `size` so the same mark renders
 * correctly at 64 px and at 1024 px.
 */
const BAR_PROFILE = [0.34, 0.62, 1.0, 0.62, 0.34];

function barsFor(size) {
  const contentWidth = size * 0.52;
  const barWidth = contentWidth / (BAR_PROFILE.length + (BAR_PROFILE.length - 1) * 0.5);
  const gap = barWidth * 0.5;
  const maxHeight = size * 0.4;
  const centre = size / 2;
  const totalWidth =
    BAR_PROFILE.length * barWidth + (BAR_PROFILE.length - 1) * gap;

  return BAR_PROFILE.map((factor, index) => {
    const x = centre - totalWidth / 2 + index * (barWidth + gap) + barWidth / 2;
    return {
      cx: x,
      cy: size / 2,
      w: barWidth,
      h: maxHeight * factor,
      r: barWidth / 2
    };
  });
}

/**
 * Rasterise one icon.
 *
 * Sampled 3x3 per pixel and averaged, which is what keeps the 64 px tray icon
 * from looking jagged without pulling in an image library.
 */
function render(size, { backdrop }) {
  const bars = barsFor(size);
  const pixels = Buffer.alloc(size * size * 4);
  const SAMPLES = 3;
  const step = 1 / SAMPLES;
  const total = SAMPLES * SAMPLES;

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;

      for (let sy = 0; sy < SAMPLES; sy++) {
        for (let sx = 0; sx < SAMPLES; sx++) {
          const px = x + (sx + 0.5) * step;
          const py = y + (sy + 0.5) * step;

          let colour = null;

          if (bars.some((bar) => inRoundedRect(px, py, bar.cx, bar.cy, bar.w, bar.h, bar.r))) {
            colour = ACCENT;
          } else if (
            backdrop &&
            inRoundedRect(px, py, size / 2, size / 2, size, size, size * 0.2)
          ) {
            colour = backdrop;
          }

          if (colour) {
            r += colour[0];
            g += colour[1];
            b += colour[2];
            a += 255;
          }
        }
      }

      const coverage = a / total; // 0..255
      const offset = (y * size + x) * 4;
      if (coverage > 0) {
        // Average only over covered samples so edges keep the true colour.
        const covered = a / 255;
        pixels[offset] = Math.round(r / covered);
        pixels[offset + 1] = Math.round(g / covered);
        pixels[offset + 2] = Math.round(b / covered);
        pixels[offset + 3] = Math.round(coverage);
      }
    }
  }
  return pixels;
}

/* -------------------------------------------------------------------- main */

mkdirSync(ICONS, { recursive: true });

const targets = [
  // Full application icon: opaque rounded square, 1024 px for `tauri icon`.
  { file: "source.png", size: 1024, options: { backdrop: BACKDROP } },
  // Tray / menu bar icon: transparent background, mark only. Coloured rather
  // than a macOS template image so it stays legible on both light and dark
  // taskbars.
  { file: "tray.png", size: 64, options: { backdrop: null } }
];

for (const { file, size, options } of targets) {
  const png = encodePng(size, size, render(size, options));
  writeFileSync(join(ICONS, file), png);
  console.log(`wrote ${file} (${size}x${size}, ${png.length} bytes)`);
}

console.log(
  "\nNext: npx tauri icon src-tauri/icons/source.png -o src-tauri/icons"
);
