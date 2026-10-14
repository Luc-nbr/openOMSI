// The drive steps' additions of phase 3 (Trello "Tauri 3/6"): their look, loaded once with the
// first of them (`hub-more.css`).

let loaded = false;

export function loadMoreCss() {
  if (loaded) return;
  loaded = true;
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = 'omsihub/hub-more.css';
  document.head.append(sheet);
}
loadMoreCss();
