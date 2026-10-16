// The duty card (Luc: "a POS-print function in company mode where you can print your duty card,
// with the IBIS data, your stops and timing points from RosterFlow"): a duty of the planning as a
// receipt printer prints it for the driver - the company's logo in black and white with its
// name, the day with its year, the driver, the duty's codes, signing on and off, and piece by
// piece every trip with its IBIS codes and the times at its timing points; the check code and a
// QR code of the duty at the foot. The Rust side says it all (`co_view` "dutycard",
// `omsihub/company/dutycard.rs`); this preview shows it at the paper's real width (58 or 80 mm,
// or A4) and prints it with the web view's own print, a page as long as the slip so that a
// receipt printer's driver cuts it there. Black on white, no colours.
//
// It opens over whatever page asks for it (the duty launcher, My duties, RosterFlow's Roster, a
// player's duties in "Play together": `relay`, the card asked of the boss's launcher).

import { render } from '../../../vendor/preact-htm.js';
import { call } from '../../api.js';
import { language } from '../../i18n.js';
import { toast } from '../../store.js';
import { html, useEffect, useRef, useState, tr, cmd, Icon, Btn, Seg, Switch } from './kit.js';

// (its look, and the company's for the buttons, loaded with it)
for (const href of ['omsihub/company.css', 'omsihub/company-dutycard.css']) {
  if (![...document.styleSheets].some((s) => s.href?.endsWith(href)) && !document.querySelector(`link[href="${href}"]`)) {
    const sheet = document.createElement('link');
    sheet.rel = 'stylesheet';
    sheet.href = href;
    document.head.append(sheet);
  }
}

/** The papers: their width (mm) and the slip's own measures on them. */
const PAPERS = { 58: { w: 58, pad: 2, logo: 34 }, 80: { w: 80, pad: 4, logo: 44 }, a4: { w: 186, pad: 4, logo: 32 } };
const KEPT = 'omsihub.dutycard';

function kept() {
  try {
    return { paper: '80', all: false, ibis: true, ...JSON.parse(localStorage.getItem(KEPT) || '{}') };
  } catch {
    return { paper: '80', all: false, ibis: true };
  }
}

function keep(o) {
  try {
    localStorage.setItem(KEPT, JSON.stringify(o));
  } catch {
    // (no storage: the choice holds while the page is open)
  }
}

let host = null;

/** Open the card of `duty` (its `date`, `line`, `tour` and `duty`); `relay`: a player of "Play
 * together" in his menu asks the boss's launcher for it. */
export function openDutyCard(duty, { relay = false } = {}) {
  if (!host) {
    host = document.createElement('div');
    host.id = 'co-dp-host';
    document.body.append(host);
  }
  const close = () => render(null, host);
  render(html`<${DutyCardDialog} duty=${duty} relay=${relay} onClose=${close} key=${JSON.stringify(duty)} />`, host);
}

/** The card's view: from this launcher, or relayed to the boss's. */
function ask(duty, relay) {
  const args = { view: 'dutycard', duty };
  if (relay) return call('tg_relay', { cmd: 'co_view', args: { ...args, lang: language() }, lang: language() });
  return cmd('co_view', args);
}

/** A picture as a receipt printer prints it: on white, black or white dots (Atkinson's
 * dithering), `mm` wide at the printer's 8 dots a millimetre. */
async function dithered(src, mm) {
  const img = new Image();
  img.src = src;
  await img.decode();
  const w = Math.round(mm * 8);
  const h = Math.max(1, Math.min(Math.round((w * img.naturalHeight) / Math.max(1, img.naturalWidth)), Math.round(w * 0.6)));
  const fit = Math.min(w / img.naturalWidth, h / img.naturalHeight);
  const cv = document.createElement('canvas');
  cv.width = w;
  cv.height = h;
  const g = cv.getContext('2d');
  g.fillStyle = '#fff';
  g.fillRect(0, 0, w, h);
  const dw = img.naturalWidth * fit;
  const dh = img.naturalHeight * fit;
  g.drawImage(img, (w - dw) / 2, (h - dh) / 2, dw, dh);
  const d = g.getImageData(0, 0, w, h);
  const px = d.data;
  const grey = new Float32Array(w * h);
  for (let i = 0; i < w * h; i++) grey[i] = 0.299 * px[i * 4] + 0.587 * px[i * 4 + 1] + 0.114 * px[i * 4 + 2];
  const spread = [[1, 0], [2, 0], [-1, 1], [0, 1], [1, 1], [0, 2]];
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      const v = grey[i] < 128 ? 0 : 255;
      const err = (grey[i] - v) / 8;
      grey[i] = v;
      for (const [dx, dy] of spread) {
        const xx = x + dx;
        const yy = y + dy;
        if (xx >= 0 && xx < w && yy < h) grey[yy * w + xx] += err;
      }
    }
  }
  for (let i = 0; i < w * h; i++) {
    px[i * 4] = px[i * 4 + 1] = px[i * 4 + 2] = grey[i];
    px[i * 4 + 3] = 255;
  }
  g.putImageData(d, 0, 0);
  return cv.toDataURL('image/png');
}

/** The company's logo in black and white (none: its short name in a box). */
function Logo({ mark, mm }) {
  const [src, setSrc] = useState(null);
  useEffect(() => {
    setSrc(null);
    if (!mark?.logo) return;
    dithered(mark.logo, mm)
      .then(setSrc)
      .catch(() => setSrc(null));
  }, [mark?.logo, mm]);
  if (src) return html`<img class="co-dp-logo" src=${src} alt="" style=${`width:${mm}mm`} />`;
  if (mark?.short) return html`<div class="co-dp-mark">${mark.short}</div>`;
  return null;
}

/** The duty's QR code (`qr`: its side and its modules row by row), drawn as one path. */
function Qr({ qr, mm }) {
  if (!qr?.bits) return null;
  const n = qr.size;
  let d = '';
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (qr.bits[y * n + x] === '1') d += `M${x + 4} ${y + 4}h1v1h-1z`;
  return html`<svg class="co-dp-qr" viewBox=${`0 0 ${n + 8} ${n + 8}`} style=${`width:${mm}mm;height:${mm}mm`} shape-rendering="crispEdges" aria-hidden="true"><rect width="100%" height="100%" fill="#fff" /><path d=${d} fill="#000" /></svg>`;
}

/** A label and its value on one line of the slip. */
const Pair = ({ a, b, c }) => html`<div class="co-dp-pair"><span>${a}</span><b>${b}</b>${c ? html`<i>${c}</i>` : null}</div>`;

/** A trip's IBIS codes: what the driver types. */
function IbisLine({ i, w }) {
  if (!i) return null;
  return html`<div class="co-dp-ibis">
    <span>IBIS</span>
    <span>${w.line_kurs} <b>${i.line_kurs}</b></span>
    ${i.route && html`<span>${w.route} <b>${i.route}</b></span>`}
    ${i.dest && html`<span>${w.dest} <b>${i.dest}</b></span>`}
    ${i.dest_text && html`<span class="co-dp-dtext">${i.dest_text}</span>`}
  </div>`;
}

/** A row of a piece. */
function Row({ r, v, all, ibis }) {
  if (r.kind === 'on' || r.kind === 'off') return html`<div class="co-dp-row strong"><span class="co-dp-t">${r.time}</span><span class="co-dp-w">${r.text}<em>${r.at}</em></span></div>`;
  if (r.kind === 'break') return html`<div class="co-dp-row"><span class="co-dp-t">${r.time}</span><span class="co-dp-w">${r.text}<em>${r.at} · ${r.until}</em></span></div>`;
  const stops = all ? r.stops : r.stops.filter((s) => s.timing);
  return html`<div class=${'co-dp-trip' + (r.empty ? ' co-dp-dead' : '')}>
    <div class="co-dp-row"><span class="co-dp-t">${r.time}</span><span class="co-dp-w">${r.empty ? html`<span>${r.line}</span>` : html`<b class="co-dp-line">${r.line}</b>`} ${r.route}<em>→ ${r.until}</em></span></div>
    ${!r.empty && ibis && html`<${IbisLine} i=${r.ibis} w=${v.ibis_words} />`}
    ${!r.empty && stops.length > 0 && html`<div class=${'co-dp-stops' + (stops.length > 8 ? ' many' : '')}>${stops.map((s) => html`<div class=${s.timing ? 'tp' : ''}><span class="co-dp-t">${s.arr ? html`<small>${s.arr}</small>` : null}${s.time}</span><span>${s.name}</span></div>`)}</div>`}
  </div>`;
}

/** The slip itself, as it is printed. */
function Slip({ v, o, printed, slipRef }) {
  const p = PAPERS[o.paper] || PAPERS[80];
  const title = v.title.toUpperCase();
  const both = title === 'DUTY CARD' ? title : `${title} / DUTY CARD`;
  return html`<div class=${'co-dp-slip co-dp-p' + o.paper} ref=${slipRef} style=${`width:${p.w}mm;padding:${p.pad + 2}mm ${p.pad}mm`}>
    <div class="co-dp-headc"><${Logo} mark=${v.mark} mm=${p.logo} /><div class="co-dp-name">${v.name}</div></div>
    <div class="co-dp-rule double"></div>
    <div class="co-dp-title">${both}</div>
    <div class="co-dp-date">${v.date}</div>
    <div class="co-dp-rule"></div>
    <div class="co-dp-cols">
      <div>
        <div class="co-dp-caps">${v.driver_caps}</div>
        ${v.driver ? html`<${Pair} a=${v.driver.name} b=${v.driver.staff || ''} />` : html`<div class="co-dp-pair"><span>—</span></div>`}
        <div class="co-dp-rule"></div>
        <div class="co-dp-caps">${v.codes_caps}</div>
        ${v.codes.map(([a, b]) => html`<${Pair} a=${a} b=${b} />`)}
      </div>
      <div>
        <div class="co-dp-rule co-dp-r2"></div>
        <div class="co-dp-caps">${v.times_caps}</div>
        ${v.times.map(([a, b, c]) => html`<${Pair} a=${a} b=${b} c=${c} />`)}
      </div>
    </div>
    ${v.pieces.map((x, k) => html`
      <div class=${'co-dp-rule' + (k === 0 ? ' double' : ' cut')}></div>
      <div class="co-dp-piece"><b>${x.head.toUpperCase()}</b><span>${x.what}</span>${x.bus && html`<span>${x.bus}</span>`}</div>
      ${x.rows.map((r) => html`<${Row} r=${r} v=${v} all=${o.all} ibis=${o.ibis} />`)}
      ${x.after && html`<div class="co-dp-rule dots"></div>
        <div class="co-dp-row strong"><span class="co-dp-t">${x.after.time}</span><span class="co-dp-w">${x.after.text}<em>${x.after.at} · ${x.after.until}</em></span></div>
        ${x.after.travel.map((t) => html`<div class="co-dp-row"><span class="co-dp-t"></span><span class="co-dp-w">${t}</span></div>`)}`}
    `)}
    <div class="co-dp-rule double"></div>
    <div class="co-dp-foot">
      <${Pair} a=${v.code_caps} b=${v.code} />
      <${Qr} qr=${v.qr} mm=${o.paper === '58' ? 22 : 26} />
      <div>${v.printed} ${printed}</div>
      <div>${v.by}</div>
    </div>
  </div>`;
}

/** The slip as a PNG (in base64): drawn as the preview shows it, every style written into it. */
async function slipPng(el) {
  const clone = el.cloneNode(true);
  const from = [el, ...el.querySelectorAll('*')];
  const to = [clone, ...clone.querySelectorAll('*')];
  from.forEach((a, k) => {
    const cs = getComputedStyle(a);
    for (const p of cs) to[k].style.setProperty(p, cs.getPropertyValue(p));
  });
  const w = el.offsetWidth;
  const h = el.offsetHeight;
  const xml = new XMLSerializer().serializeToString(clone);
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><foreignObject width="100%" height="100%">${xml}</foreignObject></svg>`;
  const img = new Image();
  img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
  await img.decode();
  const cv = document.createElement('canvas');
  const k = 3;
  cv.width = w * k;
  cv.height = h * k;
  const g = cv.getContext('2d');
  g.fillStyle = '#fff';
  g.fillRect(0, 0, cv.width, cv.height);
  g.scale(k, k);
  g.drawImage(img, 0, 0);
  return cv.toDataURL('image/png').split(',')[1];
}

/** Print the slip: the page as long as the slip (a receipt printer cuts it there), or A4. */
function printSlip(el, paper) {
  let st = document.getElementById('co-dp-page');
  if (!st) {
    st = document.createElement('style');
    st.id = 'co-dp-page';
    document.head.append(st);
  }
  const p = PAPERS[paper] || PAPERS[80];
  const long = Math.ceil((el.offsetHeight * 25.4) / 96) + 2;
  st.textContent = paper === 'a4' ? '@page { size: A4 portrait; margin: 12mm; }' : `@page { size: ${p.w}mm ${long}mm; margin: 0; }`;
  const root = document.documentElement;
  root.classList.add('co-dp-printing');
  const done = () => root.classList.remove('co-dp-printing');
  addEventListener('afterprint', done, { once: true });
  window.print();
  // (a web view whose print returns before it is done says no afterprint: the class goes on
  // the next tick after it returned)
  setTimeout(done, 1000);
}

function DutyCardDialog({ duty, relay, onClose }) {
  const [v, setV] = useState(null);
  const [o, setO] = useState(kept);
  const [printed, setPrinted] = useState('');
  const slipRef = useRef(null);
  const opt = (patch) => {
    const n = { ...o, ...patch };
    setO(n);
    keep(n);
  };
  useEffect(() => {
    let live = true;
    let timer = 0;
    const read = () =>
      ask(duty, relay)
        .then((r) => {
          if (!live) return;
          setV(r);
          // (the day still being read: asked again shortly)
          if (r?.reading) timer = setTimeout(read, 1200);
        })
        .catch((e) => live && setV({ error: e.message }));
    read();
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, []);
  useEffect(() => {
    const tick = () => setPrinted(new Date().toLocaleString(language(), { dateStyle: 'short', timeStyle: 'short' }));
    tick();
    const t = setInterval(tick, 15000);
    const k = (e) => e.key === 'Escape' && (e.stopPropagation(), onClose());
    addEventListener('keydown', k);
    return () => {
      clearInterval(t);
      removeEventListener('keydown', k);
    };
  }, []);
  const w = v?.words || {};
  const ready = v && !v.error && !v.reading && v.pieces;
  const save = async () => {
    try {
      const png = await slipPng(slipRef.current);
      const r = await call('co_dutycard_save', { png, name: `duty-${duty.date}-${duty.line}-${duty.tour}-${duty.duty + 1}.png`.replace(/[^\w.-]+/g, '_') });
      if (r?.saved) toast(r.saved, 'ok');
      else if (r?.error) toast(r.error, 'error');
    } catch (e) {
      toast(`${tr('Could not save the file')}: ${e.message || e}`, 'error');
    }
  };
  return html`<div class="co-dp-scrim" onMouseDown=${(e) => e.target === e.currentTarget && onClose()}>
    <div class="co-dp-dialog">
      <div class="co-dp-head"><${Icon} name="print_receipt" size=${22} /><h2>${w.preview || tr('Print duty card')}</h2>
        <button type="button" class="co-iconbtn" title=${w.close || tr('Close')} onClick=${onClose}><${Icon} name="close" size=${18} /></button></div>
      <div class="co-dp-body">
        <div class="co-dp-opts">
          <span class="co-dp-label">${w.paper}</span>
          <${Seg} value=${o.paper} options=${[['58', '58 mm'], ['80', '80 mm'], ['a4', w.a4 || 'A4']]} onChange=${(paper) => opt({ paper })} />
          <span class="co-dp-label">${w.stops}</span>
          <${Seg} value=${o.all ? 'all' : 'tp'} options=${[['tp', w.timing], ['all', w.all]]} onChange=${(x) => opt({ all: x === 'all' })} />
          <${Switch} checked=${o.ibis} label=${w.show_ibis} onChange=${(ibis) => opt({ ibis })} />
          <p class="co-dp-hint">${w.pdf}</p>
        </div>
        <div class="co-dp-preview">
          ${!v || v.reading ? html`<p class="co-dp-wait">${v?.reading || tr('Reading…')}</p>` : v.error ? html`<p class="co-dp-wait">${v.error}</p>` : html`<${Slip} v=${v} o=${o} printed=${printed} slipRef=${slipRef} />`}
        </div>
      </div>
      <div class="co-dp-actions">
        <${Btn} kind="primary" icon="print_receipt" label=${w.print || tr('Print')} disabled=${!ready} onClick=${() => printSlip(slipRef.current, o.paper)} />
        <${Btn} icon="download" label=${w.image || tr('Save as image')} disabled=${!ready} onClick=${save} />
        <span class="co-dp-grow"></span>
        <${Btn} label=${w.close || tr('Close')} onClick=${onClose} />
      </div>
    </div>
  </div>`;
}
