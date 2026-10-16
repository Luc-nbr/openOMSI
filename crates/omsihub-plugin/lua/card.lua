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
local GREEN, AMBER, RED, DIM = "#3FB950", "#F0A030", "#E5534B", "#9AA3AD"

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

local function ink(d)
  if not d then return DIM end
  if d > career.LATE_ARRIVAL then return RED end
  if d < career.EARLY_DEPARTURE then return AMBER end
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

-- the stops as the card's second tab lists them
local function stop_rows(run)
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
      last and "" or (v.left and hhmm(v.left) or "–"), offset(decides),
    }
  end
  return rows
end

local function panel(s)
  local run, e = s.run, s.judged
  local timed = not run.free and run.stops > 0
  local children = {
    { type = "row", children = {
      { type = "badge", text = run.line ~= "" and run.line or "–", visible = run.line ~= "" },
      { type = "text", text = run.terminus, weight = "bold", grow = true, wrap = false },
      { type = "button", id = "close", icon = "close", text = "" },
    } },
    { type = "text", size = 12, color = DIM, text = (run.free and tr("Free drive") or tr("trip %{k} of %{n}", { k = run.trip, n = math.max(run.trips, run.trip) })) .. "  ·  " .. hhmm(run.departure) .. " – " .. hhmm(run.arrival) .. "  ·  " .. tr(run.completed and "Trip completed" or "Trip ended") },
    { type = "tabs", id = "tab", tabs = { tr("Overview"), tr("Stops") }, selected = s.tab, visible = timed },
  }
  local function add(x) children[#children + 1] = x end
  if s.tab == 2 and timed then
    add({ type = "table", columns = { tr("Bus stop"), tr("Due"), tr("Arr."), tr("Dep."), "±" }, widths = { 0.4, 0.15, 0.15, 0.15, 0.15 }, rows = stop_rows(run), size = 12 })
  else
    if timed then
      local on = math.max(run.stops - run.early - run.late, 0)
      local percent = career.round(on * 100 / run.stops)
      add({ type = "row", children = {
        { type = "text", text = tr("Punctuality"), weight = "bold", grow = true },
        { type = "text", text = string.format("%d%%", percent), weight = "bold", size = 18, color = percent >= 80 and GREEN or (percent >= 50 and AMBER or RED) },
      } })
      add({ type = "bar", value = on / run.stops, color = GREEN })
      add({ type = "text", size = 12, color = DIM, text = tr("%{good} of %{all} stops on time", { good = on, all = run.stops }) .. "  ·  " .. tr("Early") .. " " .. run.early .. "  ·  " .. tr("Late") .. " " .. run.late })
      add({ type = "row", children = { tile(offset(run.worst), ink(run.worst), "Worst delay"), tile(offset(run.average), ink(run.average), "Average delay") } })
      add({ type = "row", children = { tile(duration(run.seconds), nil, "Driving time"), tile(string.format("%.1f km", run.metres / 1000), nil, "Distance") } })
    else
      add({ type = "text", text = tr(run.free and "No timetable: nothing to be early or late for" or "No stop of this trip was served"), size = 12, color = DIM })
      add({ type = "row", children = { tile(tostring(run.stops), nil, "Stops served"), tile(duration(run.seconds), nil, "Driving time"), tile(string.format("%.1f km", run.metres / 1000), nil, "Distance") } })
    end
    add({ type = "text", size = 12, color = DIM, text = tr("Jolts") .. " " .. run.jolts .. "  ·  " .. tr("Collisions") .. " " .. run.crashes .. "  ·  " .. tr("Passengers") .. " " .. run.passengers })
    add({ type = "divider" })
    add({ type = "row", children = {
      { type = "badge", text = tr(e.grade), color = score_ink(e.score) },
      { type = "text", text = string.rep("★", e.stars) .. string.rep("☆", 5 - e.stars), color = score_ink(e.score), grow = true },
      { type = "text", text = string.format("%d / 100", e.score), weight = "bold" },
      { type = "text", text = tr("+%{n} XP", { n = e.xp }), color = GREEN },
    } })
    local parts = {}
    if e.punctuality then parts[#parts + 1] = tr("Punctuality") .. " " .. e.punctuality end
    parts[#parts + 1] = tr("Comfort") .. " " .. e.comfort
    parts[#parts + 1] = tr("Safety") .. " " .. e.safety
    add({ type = "text", size = 12, text = table.concat(parts, "  ·  ") })
    local fines = run.fines or 0
    if fines > 0 then
      local what = {}
      if (run.red_lights or 0) > 0 then what[#what + 1] = tr(run.red_lights == 1 and "%{n} red light" or "%{n} red lights", { n = run.red_lights }) end
      if (run.speeding or 0) > 0 then what[#what + 1] = tr(run.speeding == 1 and "%{n} speed camera" or "%{n} speed cameras", { n = run.speeding }) end
      add({ type = "text", size = 12, color = RED, text = tr("Fines %{amount}", { amount = euros(fines) }) .. "  ·  " .. table.concat(what, ", ") })
    else
      add({ type = "text", size = 12, color = DIM, text = tr("No fines") })
    end
  end
  add({ type = "text", size = 11, color = DIM, text = (s.saved and tr("Saved to your service record") or tr("The trip could not be saved")) .. "  ·  " .. (s.pinned and tr("Enter or × to close") or tr("Click to keep it open")) })
  omsi.ui.set(PANEL, { anchor = "top", x = 0, y = 70, width = 400, accent = score_ink(e.score), clickable = not s.pinned, children = children })
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
    if shown.left <= 0 then M.close() end
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

function M.key(key, down)
  if not (shown and down) then return false end
  if key == "Enter" or key == "NumpadEnter" or (key == "Escape" and shown.pinned) then return M.close() end
  return false
end

return M
