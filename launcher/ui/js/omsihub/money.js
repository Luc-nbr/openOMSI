// The company's money as the pages write it themselves (Beta 62): in the company's currency,
// as `words::eur` writes it. The shell hands its look (`company::shell`'s `money`: the code, the
// rate to the euro, the symbol and where it goes, the marks, a fine amount's decimals); before a
// company is open, the euro as the page's language writes it. Money is kept in euro cents.

import { S } from '../store.js';
import { language } from '../i18n.js';

/** The look of the company's money (`words::money_json`). */
function look() {
  const m = S.co?.shell?.money;
  if (m) return m;
  const lang = language();
  const [thousands, decimal, at] =
    lang === '' || lang === 'en' ? [',', '.', 'before'] : lang === 'nl' ? ['.', ',', 'spaced'] : ['de', 'it', 'es', 'pt', 'pt-pt', 'tr', 'da', 'id'].includes(lang) ? ['.', ',', 'after'] : [' ', ',', 'after'];
  return { code: 'EUR', rate: 1, symbol: '€', at, thousands, decimal, places: 2 };
}

/** Euro cents in units of the company's currency. */
export function units(cents) {
  return ((Number(cents) || 0) / 100) * look().rate;
}

/** An amount in units of the company's currency (a field's, a slider's) as euro cents. */
export function cents(x) {
  return Math.round(((Number(x) || 0) * 100) / look().rate);
}

/** Money in whole units of the company's currency ("€1,234", "£1,061", "453 125 Ft"), or with
 * its decimals when `fine` (a fare; none in a currency without them). */
export function money(c, fine = false) {
  const l = look();
  return write(fine ? units(c) : Math.round(units(c)), fine ? l.places : 0);
}

/** A fare in the company's currency at the step its fares are priced in (`fare_step`: 5p, 5
 * cents, ten forints - £2.40, never £2.39), with its decimals. */
export function fare(c) {
  const l = look();
  const step = l.fare_step || 0.05;
  const u = units(c);
  return write(u > 0 ? Math.max(step, Math.round(u / step) * step) : 0, l.places);
}

/** An amount in units of the company's currency written as it writes money. */
function write(x, places) {
  const l = look();
  const text = Math.abs(x).toFixed(places);
  const [whole, frac] = text.split('.');
  let n = '';
  for (let i = 0; i < whole.length; i++) {
    if (i > 0 && (whole.length - i) % 3 === 0) n += l.thousands;
    n += whole[i];
  }
  if (frac) n += l.decimal + frac;
  const sign = x < 0 && /[1-9]/.test(text) ? '-' : '';
  return l.at === 'before' ? `${sign}${l.symbol}${n}` : l.at === 'spaced' ? `${sign}${l.symbol} ${n}` : `${sign}${n} ${l.symbol}`;
}

/** Thousands of the company's currency, short ("245k"). */
export function thousands(c) {
  return `${Math.round(units(c) / 1000)}k`;
}
