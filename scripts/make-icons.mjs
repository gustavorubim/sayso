// Draws the Sayso icon procedurally and writes PNGs — no image tooling needed.
//   build/icon.png      1024px, used by electron-builder for installers
//   resources/icon.png   256px, window icon and in-app mark
//   resources/tray.png    32px, tray icon
//
// The mark is a heavy rounded "S" that doubles as a speech bubble: a tail hangs from its
// lower-left corner and a small notch is cut into its upper-right end.
import { mkdirSync, writeFileSync } from 'node:fs'
import { deflateSync } from 'node:zlib'

const TILE = [233, 74, 53] // #E94A35
const TILE_SHADE = [206, 58, 40]
const GLYPH = [255, 237, 206] // #FFEDCE

// ---- geometry, in a 0..1 square with y pointing down ----

const R = 0.118 // radius of the S's two bowls (centre line)
const W = 0.094 // half the stroke width
const CX = 0.5
const TOP = 0.5 - 2 * R
const BOTTOM = 0.5 + 2 * R
const LEFT = CX - 0.085
const RIGHT = CX + 0.085

function arc(cx, cy, from, to, steps = 28) {
  const pts = []
  for (let i = 0; i <= steps; i++) {
    const a = ((from + ((to - from) * i) / steps) * Math.PI) / 180
    pts.push([cx + R * Math.cos(a), cy + R * Math.sin(a)])
  }
  return pts
}

// Centre line of the S, from its upper-right end to its lower-left end.
const SPINE = [
  [RIGHT + 0.07, TOP],
  ...arc(LEFT, TOP + R, 270, 90), // upper bowl, bulging left
  ...arc(RIGHT, BOTTOM - R, 270, 450), // lower bowl, bulging right
  [LEFT - 0.07, BOTTOM]
]

// Speech-bubble tail under the lower-left end.
const TAIL = [
  [LEFT - 0.115, BOTTOM + 0.03],
  [LEFT - 0.105, BOTTOM + W + 0.075],
  [LEFT + 0.03, BOTTOM + W - 0.02]
]

// Notch at the upper-right end, a small bite out of the underside.
const NOTCH = { x: RIGHT + 0.105, y: TOP + W + 0.012, rx: 0.018, ry: 0.034 }

function distToSegment(px, py, [ax, ay], [bx, by]) {
  const dx = bx - ax
  const dy = by - ay
  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy || 1)))
  return Math.hypot(px - (ax + dx * t), py - (ay + dy * t))
}

function distToSpine(px, py) {
  let d = Infinity
  for (let i = 0; i < SPINE.length - 1; i++) d = Math.min(d, distToSegment(px, py, SPINE[i], SPINE[i + 1]))
  return d
}

// Signed distance to a triangle (negative inside).
function triangle(px, py, [a, b, c]) {
  const edges = [
    [a, b],
    [b, c],
    [c, a]
  ]
  let d = Infinity
  let inside = true
  const sign = (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0]) > 0 ? 1 : -1
  for (const [p, q] of edges) {
    d = Math.min(d, distToSegment(px, py, p, q))
    const cross = (px - p[0]) * (q[1] - p[1]) - (py - p[1]) * (q[0] - p[0])
    if (cross * sign < 0) inside = false
  }
  return inside ? -d : d
}

function roundedSquare(px, py, half, r) {
  const qx = Math.abs(px - 0.5) - (half - r)
  const qy = Math.abs(py - 0.5) - (half - r)
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r
}

function glyphDistance(px, py) {
  // Union of the stroke and the tail, with a soft join; then subtract the notch.
  const stroke = distToSpine(px, py) - W
  const tail = triangle(px, py, TAIL) - 0.012
  const body = Math.min(stroke, tail)
  const nx = (px - NOTCH.x) / NOTCH.rx
  const ny = (py - NOTCH.y) / NOTCH.ry
  const notch = (Math.hypot(nx, ny) - 1) * Math.min(NOTCH.rx, NOTCH.ry)
  return Math.max(body, -notch)
}

const mix = (a, b, t) => a.map((c, i) => c + (b[i] - c) * t)

function render(size) {
  const px = new Uint8Array(size * size * 4)
  const cover = (d) => Math.min(1, Math.max(0, 0.5 - d * size))
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const u = (x + 0.5) / size
      const v = (y + 0.5) / size
      const tile = cover(roundedSquare(u, v, 0.46, 0.21))
      // A faint darker pool under the glyph gives the flat shapes a little depth.
      const shadow = Math.max(0, 1 - Math.max(0, glyphDistance(u - 0.006, v - 0.014)) / 0.035) * 0.55
      let color = mix(TILE, TILE_SHADE, shadow)
      color = mix(color, GLYPH, cover(glyphDistance(u, v)))
      const i = (y * size + x) * 4
      px[i] = Math.round(color[0])
      px[i + 1] = Math.round(color[1])
      px[i + 2] = Math.round(color[2])
      px[i + 3] = Math.round(tile * 255)
    }
  }
  return px
}

// ---- minimal PNG encoder ----

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
