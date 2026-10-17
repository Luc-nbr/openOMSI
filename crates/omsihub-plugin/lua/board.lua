-- Omsi-Hub's plugin: the duty board, as Omsi-Hub's own game drew it under the navigator
-- (omsihub-game's `nav_duty`): the trip under way with its line plate, where it goes and how it
-- stands against the timetable on a chip; the stop just behind the bus and the ones ahead on a
-- rail with their planned times (and, off the timetable, when the bus will be there); then what
-- comes after this trip - the break at the terminus, and the line and tour the duty goes on with
-- when they change. During the break the board looks ahead to the next trip. Under it the
-- company's parts (`company.lua`): the punctuality coach, the duty strip, the fare box and the
-- line's contract.
--
-- In an openOMSI with the navigator's board (`omsi.ui.version` 3) the board is in the navigator,
-- under its next-stop bar, as before: Shift+N and the handle on the bar show and hide it, and the
-- navigator's own list of stops steps aside for it. An older openOMSI gets it as a panel of its
-- own on the left, which the player can move while the panels have the mouse (F10).

local career = require("career")
local company = require("company")

local M = {}

local PANEL = "omsihub_board"
local AHEAD = 4
-- Omsi-Hub's colours (the launcher's dark theme): the line's plate, punctuality as fills (a
-- chip) and as letters, the advice's tint and ink, the texts
local PLATE, ON_PLATE = "#FFD23F", "#1A1400"
local LATE, ON_TIME, EARLY, FIELD = "#D93A30", "#1A8A4F", "#2A75F7", "#1B2231"
local LATE_INK, ON_TIME_INK, EARLY_INK = "#FF5F57", "#37D67A", "#4DA3FF"
local ACCENT, ADVICE = "#F58620", "#F5E6C8"
local TEXT, DIM, FAINT = "#E8EBF2", "#959DB0", "#6E7688"

local tr = function(t) return t end
local shown = false
-- (how many stops ahead fit in the navigator's room: fewer when the board came out too tall)
local fit = AHEAD

function M.setup(translate)
  tr = translate
  company.setup(translate)
end

local function hhmm(t)
  local m = math.floor((t or 0) / 60)
  return string.format("%02d:%02d", (m // 60) % 24, m % 60)
end

local function offset(d)
  local s = career.round(math.abs(d))
  return string.format("%s%d:%02d", d < 0 and "−" or "+", s // 60, s % 60)
end

-- whole minutes, rounded up (a break with 20 s left is not over)
local function minutes_up(s)
  return math.max(math.ceil(s / 60), 0)
end

-- OMSI 2's punctuality: more than three minutes late, more than two early
local function punctuality(d)
  if d > career.LATE_ARRIVAL then return "late" end
  if d < career.EARLY_DEPARTURE then return "early" end
  return "on_time"
end

local INK = { late = LATE_INK, early = EARLY_INK, on_time = ON_TIME_INK }

-- (the line as the trip's displays show it; "5 Rathaus" is the line's terminus Rathaus)
local function terminus_of(line, terminus)
  terminus = terminus or ""
  if line ~= "" and terminus:sub(1, #line + 1) == line .. " " then terminus = terminus:sub(#line + 2) end
  return terminus
end

-- how the duty stands: the chip's text, its fill and ink, and the phase
local function status(d, trips, clock)
  if d.trip_done and d.trip >= #trips then return tr("Duty finished"), ON_TIME, "#FFFFFF", "finished" end
  local next_trip = trips[d.trip + 1]
  if d.trip_done and next_trip then
    local left = (next_trip.departure or 0) - clock
    if left >= 0 then return tr("break, %{m} min left", { m = minutes_up(left) }), FIELD, TEXT, "break" end
    return tr("%{m} min over", { m = math.max(minutes_up(-left), 1) }), LATE, "#FFFFFF", "break"
  end
  local first = d.next_stop and d.next_stop.number == 1
  if first and clock < (d.departure or 0) then
    return tr("leaves in %{m} min", { m = math.max(minutes_up(d.departure - clock), 1) }), FIELD, TEXT, "before"
  end
  local delay = d.delay or 0
  local p = punctuality(delay)
  if p == "on_time" then return tr("on time"), ON_TIME, "#FFFFFF", "running" end
  return offset(delay), p == "late" and LATE or EARLY, "#FFFFFF", "running"
end

-- the line's plate (an empty run: a tag saying so)
local function plate(line)
  if line ~= "" then return { type = "badge", text = line, color = PLATE, text_color = ON_PLATE, weight = "bold" } end
  return { type = "badge", text = tr("Empty run"), color = "#FFFFFF22", text_color = DIM }
end

-- the panel's elements for the duty `d` (`duty.get`), its trips, the stops of the trip now and
-- the game's clock; `ahead` stops from the one the bus heads for (four at most). `old`: for an
-- openOMSI without boxes and stop lists (a table in their place).
function M.rows(d, trips, stops, clock, ahead, old)
  ahead = math.max(math.min(ahead or AHEAD, AHEAD), 1)
  local chip, fill, ink, phase = status(d, trips, clock)
  local k = d.trip
  if phase == "break" then k = d.trip + 1 end
  local trip = trips[k] or {}
  local line = trip.line or ""
  local when = phase == "break" and "leaves %{time}" or "arrives %{time}"
  local children = {
    { type = "row", gap = 8, children = {
      plate(line),
      { type = "text", text = terminus_of(line, trip.terminus), size = 14, weight = "bold", color = TEXT, grow = true, wrap = false },
      { type = "badge", text = chip, color = fill, text_color = ink, weight = "bold" },
    } },
    { type = "text", size = 11.5, weight = "medium", color = DIM, wrap = false,
      text = tr("trip %{k} of %{n}", { k = k, n = #trips }) .. "  ·  " .. tr(when, { time = hhmm(phase == "break" and trip.departure or trip.arrival) }) },
  }
  local function add(x) children[#children + 1] = x end
  local function advice(icon, text, tint)
    if old then
      add({ type = "text", size = 13, color = tint, text = text })
      return
    end
    add({ type = "box", color = tint, children = { { type = "row", gap = 8, children = {
      { type = "icon", name = icon, size = 15, color = tint },
      { type = "text", text = text, size = 12.5, weight = "bold", color = ADVICE, grow = true },
    } } } })
  end
  if phase == "finished" then
    advice("check_circle", tr("Duty finished"), ON_TIME_INK)
    return children
  end
  if phase == "break" then
    local this, after = trips[d.trip] or {}, trips[d.trip + 1] or {}
    if (after.line or "") ~= (this.line or "") then advice("sync_alt", tr("Continue on line %{line}, tour %{tour}", { line = after.line or "", tour = d.tour or "" }), ACCENT) end
    return children
  end
  -- the stop behind the bus and the ones ahead (when the bus is off the timetable, when it
  -- will be at each)
  local served, now = {}, nil
  for _, s in ipairs(stops) do
    if s.stops ~= false then
      served[#served + 1] = s
      if now == nil and not s.passed then now = #served end
    end
  end
  now = math.min(now or #served, #served)
  local late = phase == "running" and punctuality(d.delay or 0) ~= "on_time" and (d.delay or 0) or nil
  local from, to = math.max(now - 1, 1), math.min(now + ahead - 1, #served)
  local list = {}
  for j = from, to do
    local s = served[j]
    local state = j < now and "done" or (j == now and "now" or "ahead")
    local expected = (late and (state == "ahead" or (state == "now" and not d.at_stop))) and hhmm(s.arrival + late) or ""
    list[#list + 1] = {
      name = s.name or "", time = hhmm(s.arrival), expected = expected, expected_color = expected ~= "" and INK[punctuality(late)] or nil,
      state = state, tag = j == #served and tr("terminus") or "",
    }
  end
  if #list > 0 then
    if old then
      local rows = {}
      for _, s in ipairs(list) do rows[#rows + 1] = { (s.state == "now" and "▶ " or "") .. s.name, s.time, s.expected } end
      add({ type = "table", columns = {}, widths = { 0.6, 0.2, 0.2 }, rows = rows, size = 13 })
    else
      add({ type = "stops", stops = list, color = ACCENT, size = 13 })
    end
  end
  if to < #served then
    add({ type = "row", gap = 0, children = {
      { type = "space", size = old and 0 or 19 },
      { type = "text", size = 11.5, weight = "medium", color = FAINT, grow = true, text = tr("and %{n} more to %{terminus}", { n = #served - to, terminus = terminus_of(line, trip.terminus) }) },
    } })
  end
  local after = trips[d.trip + 1]
  if after then
    local pause = math.floor(((after.departure or 0) - (trip.arrival or 0)) / 60)
    local parts = {}
    if pause > 0 then parts[#parts + 1] = tr("%{m} min break", { m = pause }) end
    if (after.line or "") ~= line and (after.line or "") ~= "" then parts[#parts + 1] = tr("Continue on line %{line}, tour %{tour}", { line = after.line, tour = d.tour or "" }) end
    if old then
      local text = tr("Next %{time}", { time = hhmm(after.departure) }) .. "  ·  " .. terminus_of(after.line or "", after.terminus)
      if #parts > 0 then text = text .. "  ·  " .. table.concat(parts, "  ·  ") end
      add({ type = "text", size = 12, color = DIM, text = text })
    else
      local box = { { type = "row", gap = 7, children = {
        { type = "text", text = tr("Next %{time}", { time = hhmm(after.departure) }), size = 12.5, weight = "bold", color = ADVICE },
        plate(after.line or ""),
        { type = "text", text = terminus_of(after.line or "", after.terminus), size = 12.5, weight = "medium", color = ADVICE, grow = true, wrap = false },
      } } }
      if #parts > 0 then box[2] = { type = "text", text = table.concat(parts, "  ·  "), size = 11.5, weight = "medium", color = ADVICE .. "D9" } end
      add({ type = "box", color = ACCENT, children = box })
    end
  end
  return children
end

-- the board now (`on`: wanted; none without a duty). `parts`: the company's parts switched on,
-- `assignment` the duty Omsi-Hub gave, `boarded` the passengers of this trip and of the duty.
function M.update(on, parts, assignment, boarded)
  if not (omsi.ui and omsi.ui.set and omsi.duty and omsi.duty.get) then return end
  local d = on and omsi.duty.get() or nil
  if not d then
    if shown then
      omsi.ui.remove(PANEL)
      shown = false
    end
    return
  end
  local info = omsi.info and omsi.info() or {}
  local clock = type(info) == "table" and info.clock or 0
  local docks = (tonumber(omsi.ui.version) or 0) >= 3
  -- (in the navigator: as many stops as there is room for, found from how tall it came out)
  if docks and omsi.ui.dock then
    local _, room, h = omsi.ui.dock()
    if room and h and h > 0 then
      if h > room and fit > 1 then
        fit = fit - 1
      elseif h + 26 < room and fit < AHEAD then
        fit = fit + 1
      end
    end
  end
  local trips = omsi.duty.trips()
  local children = M.rows(d, trips, omsi.duty.stops(), clock, fit, not docks)
  if docks then
    for _, x in ipairs(company.elements(parts or {}, assignment, d, trips, clock, boarded, fit > 1)) do children[#children + 1] = x end
    omsi.ui.set(PANEL, { anchor = "navigator", padding = 11, gap = 5, children = children })
  else
    omsi.ui.set(PANEL, { anchor = "left", x = 16, y = 0, width = 340, background = "#141A26F2", accent = ACCENT, draggable = true, children = children })
  end
  shown = true
end

return M
