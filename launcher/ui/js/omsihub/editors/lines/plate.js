// A line's colour and the ink on it (`ownlines::ink_on`): the line's dark ink on a light
// colour, white on a dark one.

/** `#rrggbb` as [r, g, b] in 0..1 (the line editor's blue when it is none). */
export function rgbOf(hex) {
  const h = String(hex || '').trim().replace(/^#/, '');
  const n = /^[0-9a-f]{6}$/i.test(h) ? parseInt(h, 16) : 0x2a75f7;
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

/** The ink on the colour `hex`. */
export function inkOn(hex) {
  const [r, g, b] = rgbOf(hex);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.6 ? '#1a1400' : '#ffffff';
}
