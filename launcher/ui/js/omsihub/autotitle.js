// Words shortened with an ellipsis show the whole of them as a tooltip (Beta 53's layout rules:
// "long names shorten with an ellipsis", and the name is still there to read): when the mouse
// comes over a part whose words are cut off by an ellipsis and that has no tooltip of its own, it
// gets its words as one. Installed once for the whole page (`hub.js`); the layout checker counts
// an ellipsis as read in full so (`layout.js`).

/** Whether `el`'s words are cut off by an ellipsis now. */
function shortened(el) {
  if (!el || el.nodeType !== 1 || el.title || el.closest?.('[title]')) return false;
  const cs = getComputedStyle(el);
  return cs.textOverflow === 'ellipsis' && el.scrollWidth > el.clientWidth + 1;
}

let installed = false;

export function installAutoTitles() {
  if (installed) return;
  installed = true;
  window.hubAutoTitle = true;
  addEventListener(
    'mouseover',
    (e) => {
      for (let el = e.target, k = 0; el && k < 3; el = el.parentElement, k++) {
        if (shortened(el)) {
          el.title = el.textContent.replace(/\s+/g, ' ').trim();
          return;
        }
      }
    },
    true,
  );
}
