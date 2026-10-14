// The bus company's part of the page's self-check (`js/omsihub/selftest.js`): its start, every
// page of a company of the driver's (Normal's or Casual's) drawn without a script error, the
// dialogs, the hand-overs' page names, and the duty launcher's start as a dry run - the command
// line a duty becomes, nothing started. Nothing is bought, founded or simulated: the clock is
// held while the check runs (company.js: `still`).

import { S, set } from '../../store.js';
import { HOME, CASUAL, setCo, setUi, cmd, act } from './kit.js';
import { enterCompany, leaveCompany } from './company.js';
import { startDuty } from './launch.js';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const $ = (q) => document.querySelector(q);
const $$ = (q) => document.querySelectorAll(q);

export async function selftest(check, errors) {
  const shown = async (what, patch, ms = 900) => {
    const before = errors.length;
    if (patch) setCo(patch);
    await sleep(ms);
    return errors.length === before ? null : `${what}: ${errors.slice(before).join('; ')}`;
  };

  // the start: how to play
  enterCompany();
  let e = await shown('start', null, 1500);
  check('company: the start', !e && $$('.co-mode').length === 2, e || `${$$('.co-mode').length} ways to play`);
  const list = S.co.list?.companies?.tiles || [];
  e = await shown('companies', { chooser: 'companies' });
  check('company: the companies', !e && $$('.co-company').length === list.length + 1, e || `${list.length} companies and the new one`);
  if (!list.length) {
    check('company: a company to look at', true, 'none in this profile: the pages are not drawn (nothing is founded by the check)');
    await leaveCompany();
    return;
  }

  // a company of the driver's, every page of it
  await cmd('co_open', { id: list[0].id });
  e = await shown('the company', { chooser: null, tab: HOME }, 1500);
  const sh = S.co.shell;
  check('company: its bar', !e && !!$('.co-bar .name') && !!$('.co-status') && !!$('.co-clock'), e || `${sh?.name} (${sh?.mode?.label}) ${sh?.clock?.date} ${sh?.clock?.time}`);
  const tabs = sh?.casual ? [HOME, CASUAL.BUSES, CASUAL.SHOP, CASUAL.LINES, CASUAL.NEW_LINE, CASUAL.DRIVERS, CASUAL.MONEY, CASUAL.COMPANY, CASUAL.SHIFTS] : [HOME, 0, 1, 2, 3, 4, 6, 13];
  for (const tab of tabs) {
    e = await shown(`page ${tab}`, { tab, dialog: null });
    check(`company: page ${sh?.casual ? sh.casual_pages?.pages?.[tab]?.name : sh?.names?.[tab] || 'Start screen'}`, !e && !!$('.co-body')?.firstElementChild, e || `${$$('.co-body *').length} elements`);
  }
  if (!sh?.casual) {
    for (const [tab, page, key, n] of [
      [1, 'fleet', 'tab', 3],
      [2, 'people', 'tab', 5],
      [6, 'career', 'part', 6],
    ]) {
      for (let k = 1; k < n; k++) {
        setUi(page, { [key]: k, open: null });
        e = await shown(`${page} ${k}`, { tab });
        check(`company: ${page}, part ${k}`, !e, e || 'drawn');
      }
      setUi(page, { [key]: 0 });
    }
  }
  // the pages of phase 3 (`selftest-more.js`)
  await (await import('./selftest-more.js')).selftest(check, errors, shown);

  // the dialogs: the settings, the clock's steps
  for (const kind of ['settings', 'time']) {
    e = await shown(kind, { tab: HOME, dialog: { kind } });
    check(`company: the ${kind} dialog`, !e && !!$('.co-dialog'), e || $('.co-dialog h2')?.textContent || '');
  }
  setCo({ dialog: null });

  // the hand-overs' pages, as our game-drawn launcher reads them
  try {
    const a = await cmd('co_hand_over', { page: '5' });
    const b = await cmd('co_hand_over', { page: 'map-line=A:1' });
    check('company: the hand-over pages', a.page === `bus company:5:${sh.id}` && b.page === `bus company:map-line=A%3A1:${sh.id}`, `${a.page} | ${b.page}`);
  } catch (x) {
    check('company: the hand-over pages', false, x.message);
  }

  // the company read again from its file, as after one of our editors wrote it
  try {
    const r = await cmd('co_reread', { id: sh.id });
    e = await shown('read again', null, 600);
    check('company: read again from its file', !e && r.open === sh.id && S.co.shell?.id === sh.id, e || r.open);
  } catch (x) {
    check('company: read again from its file', false, x.message);
  }

  // the duty launcher: a duty of today's opened, its start as the command line only
  try {
    const v = await cmd('co_view', { view: 'casual', page: 'shifts' });
    const duty = v.cards?.[0]?.duty;
    if (!duty) {
      check('company: the duty launcher', true, v.reading || 'no shift today: nothing to start');
    } else {
      await act('drive_duty', { duty, from: HOME });
      e = await shown('the duty launcher', null, 1500);
      check('company: the duty launcher', !e && !!$('.co-launch'), e || $('.co-launch .head .who')?.textContent || '');
      const r = await startDuty(true);
      const line = (r?.args || []).join(' ');
      // (a duty the planning gives another driver is refused with its reason, as the start says it)
      check("company: a duty's command line", !!r?.args?.length || !!r?.popup?.title, r?.args ? line.slice(0, 240) : `refused: ${r?.popup?.title}`);
      await act('launch_close');
      setCo({ popup: null });
    }
  } catch (x) {
    check('company: the duty launcher', false, x.message);
  }

  // the workshop job the player does (Trello "Tauri 6/6"): begun, its bus and parts drawn, a
  // part checked, left again - nothing is booked
  try {
    const w = await cmd('co_view', { view: 'career', part: 3 });
    const job = w?.body?.jobs?.find((j) => j.can);
    if (!job) check('company: the workshop job', true, w?.body?.locked ? 'no workshop yet' : 'no bus to work on today');
    else {
      setUi('career', { part: 3 });
      await act('job_start', { vehicle: job.id, kind: job.kind });
      e = await shown('the workshop job', { tab: 6 }, 1200);
      const drawn = $$('.co-repair-bus .mark').length;
      await act('job_inspect', { part: 'lights' });
      const g = (await cmd('co_view', { view: 'career', part: 3 }))?.body?.game;
      check('company: the workshop job', !e && drawn === 8 && !!g?.parts?.[0]?.reading, e || `${g?.title}: ${drawn} parts, lights: ${g?.parts?.[0]?.reading}`);
      await act('job_leave');
      setUi('career', { part: 0 });
    }
  } catch (x) {
    check('company: the workshop job', false, x.message);
  }

  // the company's own tour (Trello "Tauri 6/6"): every stop finds its part
  setCo({ tab: HOME, dialog: null, popup: null });
  await sleep(500);
  await (await import('../tour.js')).checkCompanyTour(check, errors);
  // a line's timetable handed over is the line editor's without RosterFlow (its page named)
  try {
    const line = (await cmd('co_view', { view: 'lines' }))?.ours?.rows?.find((r) => r.edit_to);
    if (line) {
      const r = await cmd('co_hand_over', { page: `plan-line=${line.name}|${sh.clock?.date || ''}|1` });
      check("company: a line's timetable handed to the line editor", r.edit === line.edit_to, `${line.name}: ${r.edit}`);
    }
  } catch (x) {
    check("company: a line's timetable handed to the line editor", false, x.message);
  }

  await leaveCompany();
  set({ page: 'drive' });
  await sleep(300);
}
