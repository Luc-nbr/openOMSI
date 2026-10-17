-- Omsi-Hub's plugin: the trip's card, as Omsi-Hub's own game showed it at the end of a trip
-- (crates/omsi-app/src/trip_report.rs): the line and where it went, the stops served and how
-- many of them early, on time and late, the stop furthest off the timetable and the average,
-- the driving time, the distance, the jolts, the collisions and the passengers, how the trip is
-- judged (`career.evaluate`: a grade, its parts, the experience it earned, the fines) and, on a
-- tab of its own, each stop with when it was due and when the bus came and left. It comes when
-- the bus stands and stays some seconds of the game running; a click keeps it open until its
-- close button (or Enter, or Escape) puts it away.

local career = require("career")

local M = {}

local PANEL = "omsihub_trip"
local SHOW_FOR = 15
-- Omsi-Hub's colours (the launcher's dark theme): the sheet, its hairlines, the texts, the
-- line's plate, punctuality and the accent's ink
local GREEN, AMBER, RED, BLUE = "#37D67A", "#E8AA46", "#FF5F57", "#4DA3FF"
local SHEET, HAIRLINE = "#141A26F7", "#FFFFFF14"
local TEXT, SOFT, DIM, FAINT = "#E8EBF2", "#C4CAD6", "#959DB0", "#6E7688"
local PLATE, ON_PLATE, ACCENT_INK = "#FFD23F", "#1A1400", "#F9BC84"

local tr = function(t) return t end
local shown, waiting = nil, nil

function M.setup(translate)
  tr = translate
end

local function hhmm(t)
  local m = math.floor((t or 0) / 60)
  return string.format("%02d:%02d", (m // 60) % 24, m % 60)
end

-- how far off the timetable: +3:12, −2:30
local function offset(d)
  if not d then return "–" end
  local s = career.round(math.abs(d))
  return string.format("%s%d:%02d", d < 0 and "−" or "+", s // 60, s % 60)
end

local function ink_of(d)
  if not d then return DIM end
  if d > career.LATE_ARRIVAL then return RED end
  if d < career.EARLY_DEPARTURE then return BLUE end
  return GREEN
end

local function score_ink(score)
  if score >= 75 then return GREEN elseif score >= 50 then return AMBER end
  return RED
end

local function duration(seconds)
  local m = math.max(career.round(seconds / 60), 0)
  if m < 60 then return tr("%{m} min", { m = m }) end
  return tr("%{h}h %{m}m", { h = m // 60, m = string.format("%02d", m % 60) })
end

local function euros(cents)
  return string.format("€%d", career.round(cents / 100))
end

local function tile(value, color, label)
  return { type = "row", gap = 6, grow = true, children = {
    { type = "text", text = value, weight = "bold", size = 18, color = color, wrap = false },
    { type = "text", text = tr(label), size = 12, color = DIM, grow = true },
  } }
end

-- the stops as the card's second tab lists them (`ink`: each one's ± in the colour of how it
-- is judged)
local function stop_rows(run, ink)
  local rows = {}
  local visits = run.visits or {}
  for k, v in ipairs(visits) do
    local last = k == #visits
    local arrival = v.arrived and (v.arrived - v.arr) or nil
    local departure = (not last and v.left) and (v.left - v.dep) or nil
    local decides = arrival and career.delay(arrival, departure) or nil
    if arrival then
      local j = career.judged(arrival, departure)
      if j == "early" then decides = departure or arrival elseif j == "late" then decides = arrival end
    end
    rows[#rows + 1] = {
      v.name, hhmm(last and v.arr or v.dep), v.arrived and hhmm(v.arrived) or "–",
      last and "" or (v.left and hhmm(v.left) or "–"), ink and { text = offset(decides), color = ink_of(decides) } or offset(decides),
    }
  end
  return rows
end

-- (an openOMSI with the navigator's look: boxes, bars of several parts, the launcher's typeface)
local function looks()
  return omsi.ui and (tonumber(omsi.ui.version) or 0) >= 3
end

-- a figure of the card: its value large in its colour, what it is under it, on a tinted tile
local function figure(value, color, label)
  return { type = "box", color = color or DIM, grow = true, gap = 0, children = {
    { type = "text", text = value, weight = "bold", size = 17, color = color or TEXT, wrap = false },
    { type = "text", text = tr(label), size = 9.5, weight = "medium", color = DIM, wrap = false },
  } }
end

-- the grade's five stars, those earned in the plate's yellow (the game's typeface has no star:
-- its icons)
local function stars(n)
  local row = { type = "row", gap = 1, grow = true, children = {} }
  for i = 1, 5 do
    row.children[i] = { type = "icon", name = "star", size = 13, color = i <= n and PLATE or FAINT .. "99" }
  end
  return row
end

local function panel(s)
  local run, e = s.run, s.judged
  local timed = not run.free and run.stops > 0
  local new = looks()
  local ink = score_ink(e.score)
  local place = run.free and tr("Free drive") or tr("trip %{k} of %{n}", { k = run.trip, n = math.max(run.trips, run.trip) })
  local children = {
    { type = "row", gap = 8, children = {
      { type = "icon", name = "sports_score", size = 16, color = ink },
      { type = "text", text = tr(run.completed and "Trip completed" or "Trip ended"), size = 10.5, weight = "bold", color = DIM, grow = true, wrap = false },
      { type = "text", text = place, size = 11.5, weight = "medium", color = FAINT, wrap = false },
      { type = "button", id = "close", icon = "close", text = "" },
    } },
    { type = "row", gap = 10, children = {
      { type = "badge", text = run.line ~= "" and run.line or "–", color = PLATE, text_color = ON_PLATE, weight = "bold", visible = run.line ~= "" },
      { type = "text", text = run.terminus, weight = "bold", size = 17, color = TEXT, grow = true, wrap = false },
    } },
    { type = "text", size = 12, weight = "medium", color = DIM, text = hhmm(run.departure) .. " – " .. hhmm(run.arrival) .. "  ·  " .. duration(run.seconds) .. "  ·  " .. string.format("%.1f km", run.metres / 1000) },
    { type = "divider", color = HAIRLINE },
    { type = "tabs", id = "tab", tabs = { tr("Overview"), tr("Stops") .. " " .. #(run.visits or {}) }, selected = s.tab, visible = timed },
  }
  local function add(x) children[#children + 1] = x end
  if s.tab == 2 and timed then
    -- (the times right-aligned in their columns, the ± in its colour, where the table can)
    add({ type = "table", columns = { tr("Bus stop"), tr("Due"), tr("Arr."), tr("Dep."), "±" }, widths = { 0.4, 0.15, 0.15, 0.15, 0.15 }, rows = stop_rows(run, new), size = 12,
      aligns = new and { "left", "right", "right", "right", "right" } or nil })
  else
    if timed then
      local on = math.max(run.stops - run.early - run.late, 0)
      local percent = career.round(on * 100 / run.stops)
      local pink = percent >= 80 and GREEN or (percent >= 50 and AMBER or RED)
      add({ type = "row", gap = 12, children = {
        { type = "text", text = string.format("%d %%", percent), weight = "bold", size = 32, color = pink, wrap = false },
        { type = "box", color = "#00000000", grow = true, gap = 1, children = {
          { type = "text", text = tr("Punctuality"), size = 10, weight = "bold", color = DIM },
          { type = "text", text = tr("%{good} of %{all} stops on time", { good = on, all = run.stops }), size = 13, weight = "medium", color = SOFT },
        } },
      } })
      if new then
        add({ type = "bar", height = 8, segments = { { value = run.early, color = BLUE }, { value = on, color = GREEN }, { value = run.late, color = RED } } })
      else
        add({ type = "bar", value = on / run.stops, color = GREEN })
      end
      add({ type = "text", size = 12, color = DIM, text = tr("Early") .. " " .. run.early .. "  ·  " .. tr("on time") .. " " .. on .. "  ·  " .. tr("Late") .. " " .. run.late })
      if new then
        add({ type = "row", gap = 8, children = { figure(offset(run.worst), ink_of(run.worst), "Worst delay"), figure(offset(run.average), ink_of(run.average), "Average delay") } })
        add({ type = "row", gap = 8, children = { figure(duration(run.seconds), nil, "Driving time"), figure(string.format("%.1f km", run.metres / 1000), nil, "Distance") } })
      else
        add({ type = "row", children = { tile(offset(run.worst), ink_of(run.worst), "Worst delay"), tile(offset(run.average), ink_of(run.average), "Average delay") } })
        add({ type = "row", children = { tile(duration(run.seconds), nil, "Driving time"), tile(string.format("%.1f km", run.metres / 1000), nil, "Distance") } })
      end
    else
      add({ type = "text", text = tr(run.free and "No timetable: nothing to be early or late for" or "No stop of this trip was served"), size = 12, color = DIM })
      add({ type = "row", gap = 8, children = { (new and figure or tile)(tostring(run.stops), nil, "Stops served"), (new and figure or tile)(duration(run.seconds), nil, "Driving time"), (new and figure or tile)(string.format("%.1f km", run.metres / 1000), nil, "Distance") } })
    end
    add({ type = "text", size = 12, color = DIM, text = tr("Jolts") .. " " .. run.jolts .. "  ·  " .. tr("Collisions") .. " " .. run.crashes .. "  ·  " .. tr("Passengers") .. " " .. run.passengers })
    add({ type = "divider", color = HAIRLINE })
    add({ type = "row", gap = 8, children = {
      { type = "badge", text = tr(e.grade), color = ink, weight = "bold" },
      stars(e.stars),
      { type = "text", text = string.format("%d / 100", e.score), weight = "bold", color = TEXT },
      { type = "text", text = tr("+%{n} XP", { n = e.xp }), weight = "bold", color = ACCENT_INK },
    } })
    local parts = {}
    if e.punctuality then parts[#parts + 1] = tr("Punctuality") .. " " .. e.punctuality end
    parts[#parts + 1] = tr("Comfort") .. " " .. e.comfort
    parts[#parts + 1] = tr("Safety") .. " " .. e.safety
    add({ type = "text", size = 12, color = SOFT, text = table.concat(parts, "  ·  ") })
    local fines = run.fines or 0
    if fines > 0 then
      local what = {}
      if (run.red_lights or 0) > 0 then what[#what + 1] = tr(run.red_lights == 1 and "%{n} red light" or "%{n} red lights", { n = run.red_lights }) end
      if (run.speeding or 0) > 0 then what[#what + 1] = tr(run.speeding == 1 and "%{n} speed camera" or "%{n} speed cameras", { n = run.speeding }) end
      local text = tr("Fines %{amount}", { amount = euros(fines) })
      if #what > 0 then text = text .. "  ·  " .. table.concat(what, ", ") end
      add({ type = "text", size = 12, color = RED, text = text })
    else
      add({ type = "text", size = 12, color = DIM, text = tr("No fines") })
    end
  end
  add({ type = "divider", color = HAIRLINE })
  add({ type = "text", size = 11, color = FAINT, text = (s.saved and tr("Saved to your service record") or tr("The trip could not be saved")) .. "  ·  " .. (s.pinned and tr("Enter or × to close") or tr("Click to keep it open")) })
  -- (the time it has left, as a line along its foot)
  if not s.pinned then add({ type = "bar", id = "left", height = 2, value = math.max(s.left / SHOW_FOR, 0), color = ink, background = "#00000000" }) end
  omsi.ui.set(PANEL, { anchor = "top", x = 0, y = 70, width = 400, padding = 18, gap = 7, radius = 12, background = SHEET, font = "launcher", clickable = not s.pinned, grab = true, children = children })
end

local function show(run, saved)
  shown = { run = run, judged = career.evaluate(run), saved = saved, tab = 1, pinned = false, left = SHOW_FOR }
  panel(shown)
end

-- a trip's card: now when the bus stands, else when it next does
function M.hold(run, saved, standing)
  if not (omsi.ui and omsi.ui.set) then return end
  if standing then
    if waiting then show(waiting[1], waiting[2]) waiting = nil end
    show(run, saved)
  else
    if waiting then show(waiting[1], waiting[2]) end
    waiting = { run, saved }
  end
end

function M.close()
  if not shown then return false end
  shown = nil
  if omsi.ui and omsi.ui.remove then omsi.ui.remove(PANEL) end
  return true
end

-- every frame: the time the card has left, and a card waiting for the bus to stand
function M.frame(dt, standing)
  if standing and waiting then
    show(waiting[1], waiting[2])
    waiting = nil
  end
  if shown and not shown.pinned then
    shown.left = shown.left - dt
    if shown.left <= 0 then return M.close() end
    -- (the line along its foot shortens, four times a second)
    shown.tick = (shown.tick or 0) - dt
    if shown.tick <= 0 and omsi.ui.update then
      shown.tick = 0.25
      omsi.ui.update(PANEL, "left", { value = math.max(shown.left / SHOW_FOR, 0) })
    end
  end
end

function M.click(panel_id, element)
  if not shown or panel_id ~= PANEL then return false end
  if element == "close" then return M.close() end
  shown.pinned = true
  panel(shown)
  return true
end

function M.change(panel_id, element, value)
  if not shown or panel_id ~= PANEL or element ~= "tab" then return false end
  shown.tab, shown.pinned = tonumber(value) or 1, true
  panel(shown)
  return true
end

-- a key (`held`: Ctrl, Alt or Shift is held - Ctrl+Enter is the IBIS's, not the card's)
function M.key(key, down, held)
  if not (shown and down) or held then return false end
  if key == "Enter" or key == "NumpadEnter" or (key == "Escape" and shown.pinned) then return M.close() end
  return false
end

return M
