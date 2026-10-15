// "Play together" on for the company mode (Beta 61), with a kill switch: switched off (the
// launcher's `together::KILLED`, or OMSI_TOGETHER_OFF; OMSI_TOGETHER forces it on - see
// docs/DEBUG_FLAGS.md) its buttons are shown disabled as coming soon. The company's shell says it
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
