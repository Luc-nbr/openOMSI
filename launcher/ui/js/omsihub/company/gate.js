// "Play together" blocked for now (Luc 2026-10-15: "block the multiplayer buttons for now
// because that's still in beta"): its buttons are shown disabled as coming soon unless the
// launcher runs with OMSI_TOGETHER set (docs/DEBUG_FLAGS.md). The company's shell says it
// (`together`); outside a company the gate is asked once (`tg_gate`).

import { S } from '../../store.js';
import { call } from '../../api.js';
import { tr } from '../bits.js';

let asked = null;
call('tg_gate', {})
  .then((g) => (asked = g))
  .catch(() => {});

/** Playing together is open. */
export function togetherOn() {
  return !!(S.co?.shell?.together?.on ?? asked?.on);
}

/** What a blocked button says. */
export function blockedLabel() {
  return S.co?.shell?.together?.label || asked?.label || tr('Multiplayer - coming soon (beta)');
}
