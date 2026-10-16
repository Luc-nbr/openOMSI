-- Omsi-Hub's plugin: the duty board, as Omsi-Hub's own game drew it under the navigator
-- (omsihub-game's `nav_duty`): the trip under way with its line, where it goes and how it
-- stands against the timetable; the stop just behind the bus and the ones ahead with their
-- planned times (and, off the timetable, when the bus will be there); then what comes after
-- this trip - the break at the terminus, and the line and tour the duty goes on with when they
-- change. During the break the board looks ahead to the next trip. A panel of its own, which
-- the player can move while the panels have the mouse (F10).

local career = require("career")

local M = {}

local PANEL = "omsihub_board"
local AHEAD = 4
local GREEN, AMBER, RED, BLUE, DIM, NOW = "#37D67A", "#F0B429", "#FF5F57", "#4DA3FF", "#9AA3AD", "#F0B429"

local tr = function(t) return t end
local shown = false

function M.setup(translate)
  tr = translate
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

-- (the line as the trip's displays show it; "5 Rathaus" is the line's terminus Rathaus)
local function terminus_of(line, terminus)
  terminus = terminus or ""
  if line ~= "" and terminus:sub(1, #line + 1) == line .. " " then terminus = terminus:sub(#line + 2) end
  return terminus
end

-- how the duty stands: the chip's text and colour
local function status(d, trips, clock)
  if d.trip_done and d.trip >= #trips then return tr("Duty finished"), GREEN, "finished" end
  local next_trip = trips[d.trip + 1]
  if d.trip_done and next_trip then
    local left = (next_trip.departure or 0) - clock
    if left >= 0 then return tr("break, %{m} min left", { m = minutes_up(left) }), DIM, "break" end
    return tr("%{m} min over", { m = math.max(minutes_up(-left), 1) }), RED, "break"
  end
  local first = d.next_stop and d.next_stop.number == 1
  if first and clock < (d.departure or 0) then
    return tr("leaves in %{m} min", { m = math.max(minutes_up(d.departure - clock), 1) }), DIM, "before"
  end
  local delay = d.delay or 0
  local p = punctuality(delay)
  if p == "on_time" then return tr("on time"), GREEN, "running" end
  return offset(delay), p == "late" and RED or BLUE, "running"
end

-- the panel's elements for the duty `d` (`duty.get`), its trips and the game's clock
function M.rows(d, trips, stops, clock)
  local chip, color, phase = status(d, trips, clock)
  local k = d.trip
  if phase == "break" then k = d.trip + 1 end
  local trip = trips[k] or {}
  local line = trip.line or ""
  local children = {
    { type = "row", children = {
      { type = "badge", text = line ~= "" and line or tr("Empty run"), color = "#FFD23F", text_color = "#1A1400" },
      { type = "text", text = terminus_of(line, trip.terminus), weight = "bold", grow = true, wrap = false },
      { type = "badge", text = chip, color = color },
    } },
    { type = "text", size = 12, color = DIM, text = tr("trip %{k} of %{n}", { k = k, n = #trips }) .. "  ·  " .. hhmm(phase == "break" and trip.departure or trip.arrival) },
  }
  local function add(x) children[#children + 1] = x end
  if phase == "finished" then return children end
  if phase == "break" then
    local this, after = trips[d.trip] or {}, trips[d.trip + 1] or {}
    if (after.line or "") ~= (this.line or "") then add({ type = "text", size = 13, color = NOW, text = tr("Continue on line %{line}, tour %{tour}", { line = after.line or "", tour = d.tour or "" }) }) end
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
  local from, to = math.max(now - 1, 1), math.min(now + AHEAD - 1, #served)
  local rows = {}
  for j = from, to do
    local s = served[j]
    local state = j < now and "done" or (j == now and "now" or "ahead")
    local expected = (late and (state == "ahead" or (state == "now" and not d.at_stop))) and hhmm(s.arrival + late) or ""
    rows[#rows + 1] = { (state == "now" and "▶ " or "") .. (s.name or ""), hhmm(s.arrival), expected }
  end
  if #rows > 0 then add({ type = "table", columns = { "", "", "" }, widths = { 0.6, 0.2, 0.2 }, rows = rows, size = 13 }) end
  if to < #served then
    local n = #served - to
    add({ type = "text", size = 12, color = DIM, text = tr("and %{n} more to %{terminus}", { n = n, terminus = terminus_of(line, trip.terminus) }) })
  end
  local after = trips[d.trip + 1]
  if after then
    local pause = math.floor(((after.departure or 0) - (trip.arrival or 0)) / 60)
    local text = tr("Next %{time}", { time = hhmm(after.departure) }) .. "  ·  " .. terminus_of(after.line or "", after.terminus)
    if pause > 0 then text = text .. "  ·  " .. tr("%{m} min break", { m = pause }) end
    add({ type = "text", size = 12, color = DIM, text = text })
  end
  return children
end

-- the board now (`on`: wanted; none without a duty)
function M.update(on)
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
  local children = M.rows(d, omsi.duty.trips(), omsi.duty.stops(), type(info) == "table" and info.clock or 0)
  omsi.ui.set(PANEL, { anchor = "top_left", x = 16, y = 90, width = 330, accent = "#2A75F7", draggable = true, children = children })
  shown = true
end

return M
