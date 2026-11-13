// Draws the app icon procedurally and writes PNGs — no image tooling needed.
//   build/icon.png      1024px, used by electron-builder for installers
//   resources/icon.png   256px, window icon
//   resources/tray.png    32px, tray icon
import { mkdirSync, writeFileSync } from 'node:fs'
import { deflateSync } from 'node:zlib'

const BG = [31, 95, 74]
const BAR = [238, 241, 236]
const CURSOR = [255, 184, 77]

// Signed distance to a rounded rectangle centred at (cx, cy).
function roundedRect(px, py, cx, cy, halfW, halfH, r) {
  const qx = Math.abs(px - cx) - (halfW - r)
  const qy = Math.abs(py - cy) - (halfH - r)
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r
}

// Four sound bars, then a text cursor: speech turning into text.
const BARS = [
  { x: 0.25, h: 0.2 },
  { x: 0.375, h: 0.42 },
  { x: 0.5, h: 0.28 },
  { x: 0.625, h: 0.5 }
]

function render(size) {
  const px = new Uint8Array(size * size * 4)
  const cover = (d) => Math.min(1, Math.max(0, 0.5 - d))
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const u = x + 0.5
      const v = y + 0.5
      const bg = cover(roundedRect(u, v, size / 2, size / 2, size * 0.46, size * 0.46, size * 0.2))
      let color = BG
      let bar = 0
      for (const b of BARS) {
        const w = size * 0.036
        bar = Math.max(bar, cover(roundedRect(u, v, size * b.x, size / 2, w, (size * b.h) / 2, w)))
      }
      const cw = size * 0.022
      const cursor = cover(roundedRect(u, v, size * 0.76, size / 2, cw, size * 0.27, cw))
      const mix = (a, b, t) => a.map((c, i) => c + (b[i] - c) * t)
      color = mix(mix(color, BAR, bar), CURSOR, cursor)
      const i = (y * size + x) * 4
      px[i] = Math.round(color[0])
      px[i + 1] = Math.round(color[1])
      px[i + 2] = Math.round(color[2])
      px[i + 3] = Math.round(bg * 255)
    }
  }
  return px
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
  return c >>> 0
})
function crc32(buf) {
  let c = 0xffffffff
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}
function chunk(type, data) {
  const body = Buffer.concat([Buffer.from(type), data])
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length)
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(body))
  return Buffer.concat([len, body, crc])
}
function png(size) {
  const pixels = render(size)
  const raw = Buffer.alloc((size * 4 + 1) * size)
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0
    Buffer.from(pixels.buffer, y * size * 4, size * 4).copy(raw, y * (size * 4 + 1) + 1)
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(size, 0)
  ihdr.writeUInt32BE(size, 4)
  ihdr.set([8, 6, 0, 0, 0], 8)
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0))
  ])
}

mkdirSync('build', { recursive: true })
mkdirSync('resources', { recursive: true })
writeFileSync('build/icon.png', png(1024))
writeFileSync('resources/icon.png', png(256))
writeFileSync('resources/tray.png', png(32))
console.log('icons written')
