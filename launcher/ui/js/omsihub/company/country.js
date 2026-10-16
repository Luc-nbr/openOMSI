// The country a company plays in (Beta 62): its people's names (`company/country.rs`). A field
// with the country chosen that opens a searchable list of them all - the flags, the names in the
// page's language - for the founding wizard and the company's settings.

import { html, useEffect, useRef, useState, Icon } from './kit.js';

/** A text as the list searches it: lower case, without its accents (`names::fold`). */
const fold = (s) =>
  s
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/ß/g, 'ss')
    .replace(/[łľ]/g, 'l')
    .replace(/ø/g, 'o')
    .replace(/ı/g, 'i')
    .replace(/đ/g, 'd');

/** The countries' choice (`c`: the view's `country` part - its options, the search's words):
 * the one chosen, and the list to choose another from (Enter takes the first found, Escape
 * closes it). */
export function CountryPick({ c, value, onChange, style = '' }) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState('');
  const field = useRef(null);
  useEffect(() => {
    if (open && field.current) field.current.focus();
  }, [open]);
  const chosen = c.options.find((o) => o.key === value) || c.options[0];
  const raw = q.trim().toLowerCase();
  const shown = c.options.filter((o) => !raw || o.words.includes(fold(raw)) || o.label.toLowerCase().includes(raw));
  const pick = (k) => {
    setOpen(false);
    setQ('');
    if (k !== value) onChange(k);
  };
  const key = (e) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      setOpen(false);
    } else if (e.key === 'Enter' && shown.length) {
      pick(shown[0].key);
    }
  };
  const row = 'display:flex;align-items:center;gap:10px;width:100%;height:34px;padding:0 10px;border:0;border-radius:6px;background:transparent;color:var(--hub-text);font:inherit;text-align:left;cursor:pointer';
  return html`<div style=${'position:relative;' + style}>
    <button type="button" class="co-field" style="display:flex;align-items:center;gap:10px;text-align:left;cursor:pointer" onClick=${() => setOpen(!open)}>
      <span style="font-size:18px;line-height:1">${chosen.flag}</span>
      <span style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${chosen.label}</span>
      <${Icon} name=${open ? 'expand_less' : 'expand_more'} size=${20} />
    </button>
    ${open &&
    html`<div style="position:absolute;left:0;right:0;top:44px;z-index:30;padding:8px;border-radius:var(--hub-r);background:var(--hub-panel);box-shadow:0 0 0 1px var(--hub-hairline), 0 12px 32px rgba(0, 0, 0, 0.45)">
      <label class="co-search"><${Icon} name="search" size=${18} /><input ref=${field} class="co-field" style="height:36px" value=${q} placeholder=${c.search} onInput=${(e) => setQ(e.currentTarget.value)} onKeyDown=${key} /></label>
      <div style="max-height:236px;overflow-y:auto;margin-top:6px">
        ${shown.length
          ? shown.map(
              (o) => html`<button type="button" style=${row + (o.key === value ? ';background:rgba(var(--hub-accent-rgb), 0.18);font-weight:600' : '')} onClick=${() => pick(o.key)}>
                <span style="font-size:18px;line-height:1">${o.flag}</span><span>${o.label}</span>
              </button>`
            )
          : html`<p class="co-note" style="padding:8px 10px">${c.none}</p>`}
      </div>
    </div>`}
  </div>`;
}
