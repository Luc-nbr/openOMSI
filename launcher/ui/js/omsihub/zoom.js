// Ctrl and the mouse wheel size the launcher ("Launcher size", `launcher_scale`), as our
// game-drawn launcher did (`Launcher::zoom_by`) and a browser zooms a page: 5 % a notch, 60 to
// 200 %, kept in the settings and said in a short note; Ctrl + and Ctrl - take 10 %, Ctrl 0 goes
// back to 100 % (Cmd on a Mac). WebView2's own zoom stays off (`zoom_hotkeys_enabled(false)` in
// the Tauri window), so the two never fight.
//
// RosterFlow zooms its own views with Ctrl and the wheel (its Gantt): while it is open, or where
// the wheel turns inside it, Ctrl and the wheel never size the launcher. A part of the page that
// takes Ctrl and the wheel itself (a map, the livery studio) says so by `preventDefault`, and
// is left alone too.

import { S, set } from '../store.js';
import { saveSettings } from '../data.js';
import { tr } from './bits.js';
import { rfOpen } from './rosterflow/state.js';

export const MIN = 0.6;
export const MAX = 2;
/** A notch of the wheel; a key. */
export const NOTCH = 0.05;
export const KEY_STEP = 0.1;

const MAC = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** The size as the settings keep it: 60 to 200 %, in steps of 5 % (`core::launcher_scale`). */
export const clampScale = (v) => Math.round(Math.max(MIN, Math.min(MAX, Number.isFinite(v) ? v : 1)) * 20) / 20;

/** The size now. */
export const launcherScale = () => clampScale(Number(S.settings?.launcher_scale) || 1);

let saving = 0;
let noteTimer = 0;
/** The wheel's turn not yet a whole notch (a touchpad's pinch comes as many small parts of one;
 * each taken as a step sent the launcher to its largest or smallest at once). */
let wheelLeft = 0;
const NOTE = 'hub-size';

/** Make the launcher larger or smaller by `by` (0: back to 100 %). Returns the new size. */
export function zoomBy(by) {
  const now = launcherScale();
  const to = by === 0 ? 1 : clampScale(now + by);
  if (to !== now) {
    set({ settings: { ...S.settings, launcher_scale: to } });
    clearTimeout(saving);
    saving = setTimeout(() => saveSettings({ launcher_scale: launcherScale() }), 250);
  }
  // the note: one at a time, replaced while the wheel turns
  const text = tr('Launcher size: %{pct} (Ctrl+wheel, Ctrl 0 for the normal size)', { pct: `${Math.round(to * 100)}%` });
  set({ toasts: [...S.toasts.filter((x) => x.id !== NOTE), { id: NOTE, text, kind: 'info' }] });
  clearTimeout(noteTimer);
  noteTimer = setTimeout(() => set({ toasts: S.toasts.filter((x) => x.id !== NOTE) }), 1800);
  return to;
}

/** Whether Ctrl and the wheel belong to RosterFlow here (open, or the wheel turned inside it). */
const inRosterFlow = (e) => rfOpen() || !!e.target?.closest?.('.rf');

function onWheel(e) {
  if (!(e.ctrlKey || (MAC && e.metaKey)) || !e.deltaY) return;
  // (the browser never zooms the page itself)
  e.preventDefault();
  // (handled where it turned: RosterFlow's zoom, a map's)
  if (inRosterFlow(e)) return;
  const lines = e.deltaMode === 1 ? e.deltaY * 33 : e.deltaMode === 2 ? e.deltaY * 100 : e.deltaY;
  wheelLeft += -lines / 100;
  const notches = Math.trunc(wheelLeft);
  if (!notches) return;
  wheelLeft -= notches;
  zoomBy(NOTCH * notches);
}

function onKey(e) {
  if (!(MAC ? e.metaKey : e.ctrlKey) || e.altKey) return;
  const el = document.activeElement;
  if (el && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) && e.key !== '0') return;
  const by = e.key === '+' || e.key === '=' || e.code === 'NumpadAdd' ? KEY_STEP : e.key === '-' || e.code === 'NumpadSubtract' ? -KEY_STEP : e.key === '0' || e.code === 'Numpad0' ? 0 : null;
  if (by == null) return;
  e.preventDefault();
  if (inRosterFlow(e)) return;
  zoomBy(by);
}

let installed = false;

/** Listen for Ctrl and the wheel and the keys (once). Parts that take Ctrl and the wheel
 * themselves stop it before it gets here (`defaultPrevented` on their own element first). */
export function installZoom() {
  if (installed) return;
  installed = true;
  // (a part's own handler ran first and took it: left alone)
  addEventListener('wheel', (e) => (e.defaultPrevented && (e.ctrlKey || e.metaKey) ? e.preventDefault() : onWheel(e)), { passive: false });
  addEventListener('keydown', onKey);
}
