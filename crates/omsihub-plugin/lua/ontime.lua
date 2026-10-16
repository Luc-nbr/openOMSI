-- Omsi-Hub's plugin: on time - what the bus company tells its driver on the way (Luc,
-- 2026-10-16): how the bus stands against the timetable as the company counts a trip, the gap
-- to the company's buses ahead and behind on a frequent line (and how long to hold when they
-- bunch), the protected connections at the stops ahead, and the diversions and roadworks of
-- the day - the closed stops struck through on the navigator, and a word before them.
--
-- A card of its own beside the duty board, switched off by the assignment (`ontime = false`:
-- the launcher's setting) and by its own close button for the drive. It extends what the duty
-- board and the trip's card say rather than repeating it: the board shows the game's delay
-- chip, this the company's view of the same seconds (when the trip counts late, how far off
-- that is).
--
-- The files it shares with Omsi-Hub (crates/omsi-launcher-api/src/omsihub/ontime.rs):
--   data/live.json      plugin -> Omsi-Hub: where the bus is (the trip, its stops, the delay)
--   data/ontime.json    Omsi-Hub -> plugin: the brief (headway, connections, diversions, alerts)

local json = require("hubjson")

local M = {}

local PANEL = "omsihub_ontime"
local WRITE_EVERY, READ_EVERY = 3, 3
-- a brief older than this (s) is of a game Omsi-Hub no longer looks after
local STALE = 60
local GREEN, AMBER, RED, BLUE, DIM, NOW = "#37D67A", "#F0B429", "#FF5F57", "#4DA3FF", "#9AA3AD", "#F0B429"

-- the texts Omsi-Hub's brief sends as its alerts' templates (`notify` in ontime.rs): here so
-- that they are the plugin's texts, translated with the rest
local SENT = {
  "Hold %{secs} s at %{stop}: the bus ahead is close",
  "Close behind %{bus}: no time to hold",
  "Connection at %{stop}: wait max %{m} min",
  "Diversion ahead: %{stop} closed",
  "Roadworks on the line today: expect delays",
  "Diversion on the line today: expect delays",
  "Diversion on the line today",
}
M.SENT = SENT

local tr = function(t) return t end
local assignment, has = nil, {}
local brief, closed_sent = nil, nil
local write_wait, read_wait = 0, 0
local off = false
local shown = false

local function fn(t, name)
  return type(t) == "table" and type(t[name]) == "function"
end

function M.setup(translate, a, functions)
  tr, assignment, has = translate, a, functions or {}
  off = a ~= nil and a.ontime == false
end

-- on: a company's duty, not switched off
function M.on()
  return not off and assignment ~= nil and (assignment.duty or "") ~= ""
end

local function now()
  return fn(omsi.util, "now") and omsi.util.now() or 0
end

local function clock()
  local i = fn(omsi, "info") and omsi.info() or nil
  return type(i) == "table" and i.clock or 0
end

local function hhmm(t)
  local m = math.floor((t or 0) / 60)
  return string.format("%02d:%02d", (m // 60) % 24, m % 60)
end

local function offset(d)
  local s = math.floor(math.abs(d) + 0.5)
  return string.format("%s%d:%02d", d < 0 and "−" or "+", s // 60, s % 60)
end

local function mmss(s)
  s = math.max(math.floor(s + 0.5), 0)
  return string.format("%d:%02d", s // 60, s % 60)
end

-- the company's view of a delay (`ontime::tone`): early from `early_from` s before, late from
-- `late_from` s behind
function M.tone(delay, early_from, late_from)
  if delay < -early_from then return "early" end
  if delay >= late_from then return "late" end
  if delay >= 60 then return "behind" end
  return "on_time"
end

-- --- the alerts ---------------------------------------------------------------------------

omsi.data.ontime_seen = omsi.data.ontime_seen or {}

-- The ONE way this card tells the driver something once: the game's toast for now. When the
-- company's messages come (r67-messages: `post(company, to, priority, template, args, action)`),
-- the launcher posts them (ontime.rs `notify`) and this stays the toast of a game that shows
-- what it is sent - switch here to showing the posted message instead.
local function alert(id, priority, template, args)
  if omsi.data.ontime_seen[id] then return end
  omsi.data.ontime_seen[id] = true
  local text = tr(template, args)
  if has["ui.toast"] then
    omsi.ui.toast(text, { title = tr("On time"), icon = priority == "high" and "warning" or "schedule", seconds = priority == "high" and 10 or 6 })
  else
    omsi.message(text, 6)
  end
end
M.alert = alert

-- --- the files with Omsi-Hub ----------------------------------------------------------------

-- where the bus is, for Omsi-Hub
local function write_live(d)
  if not has["files.write"] then return end
  local stops = {}
  for _, s in ipairs(fn(omsi.duty, "stops") and omsi.duty.stops() or {}) do
    stops[#stops + 1] = { name = s.name or "", arrival = s.arrival or 0, departure = s.departure or 0, stops = s.stops ~= false }
  end
  local next = d.next_stop and (d.next_stop.number or 1) - 1 or #stops
  omsi.files.write("live.json", json.encode({
    issued = assignment and tonumber(assignment.issued) or 0, at = now(), clock = clock(),
    line = d.line or "", tour = d.tour or "", trip = d.trip or 0, trip_name = d.trip_name or "",
    next = next, at_stop = d.at_stop == true, delay = d.delay or 0, stops = stops,
  }))
end

-- Omsi-Hub's brief, when it is about this drive and this trip
local function read_brief(d)
  if not (has["files.read"] and has["json.decode"]) then return nil end
  local text = omsi.files.read("ontime.json")
  if not text then return nil end
  local ok, b = pcall(omsi.json.decode, text)
  if not ok or type(b) ~= "table" then return nil end
  local mine = assignment and tonumber(assignment.issued) or 0
  if (tonumber(b.issued) or 0) ~= mine or tonumber(b.trip) ~= d.trip then return nil end
  if now() > 0 and now() - (tonumber(b.at) or 0) > STALE then return nil end
  return b
end

-- the closed stops of the day to the navigator (where the game has it)
local function to_navigator(b)
  if not fn(omsi.duty, "set_closed_stops") then return end
  local list = b and b.closed or {}
  local key = table.concat(list, "|") .. "#" .. (b and b.via or "")
  if key == closed_sent then return end
  closed_sent = key
  omsi.duty.set_closed_stops(list, b and b.via or "")
end

-- --- the card ------------------------------------------------------------------------------

local function line(text, color, size)
  return { type = "text", text = text, size = size or 13, color = color }
end

-- the card's elements for the duty `d` (`duty.get`) and the brief `b` (none yet: punctuality
-- alone, by the company's usual lines)
function M.rows(d, b)
  b = b or {}
  local early_from, late_from = tonumber(b.early_from) or 60, tonumber(b.late_from) or 360
  local delay = d.delay or 0
  local tone = M.tone(delay, early_from, late_from)
  local word = ({ early = "early", on_time = "on time", behind = "late", late = "late" })[tone]
  local color = ({ early = BLUE, on_time = GREEN, behind = AMBER, late = RED })[tone]
  local children = {
    { type = "row", children = {
      { type = "icon", name = "schedule", color = NOW },
      { type = "text", text = tr("On time"), weight = "bold", grow = true, wrap = false },
      { type = "badge", text = offset(delay) .. " " .. tr(word), color = color },
      { type = "button", id = "close", icon = "close", text = "" },
    } },
  }
  local function add(x) children[#children + 1] = x end
  -- how far off the company's late line (the board shows the game's own chip)
  if tone == "late" then
    add(line(tr("Counts as a late trip"), RED, 12))
  elseif delay > 0 then
    add(line(tr("Counts late from %{t}", { t = offset(late_from) }) .. "  ·  " .. tr("%{t} left", { t = mmss(late_from - delay) }), DIM, 12))
  elseif tone == "early" then
    add(line(tr("Running early: wait at the stop"), BLUE, 12))
  end
  -- the headway, where the line is frequent
  local h = b.headway
  if type(h) == "table" then
    add({ type = "divider" })
    local parts = {}
    if type(h.ahead) == "table" then parts[#parts + 1] = tr("Ahead %{bus}: %{gap}", { bus = h.ahead.bus, gap = mmss(h.ahead.secs) }) end
    if type(h.behind) == "table" then parts[#parts + 1] = tr("Behind %{bus}: %{gap}", { bus = h.behind.bus, gap = mmss(h.behind.secs) }) end
    parts[#parts + 1] = tr("every %{m} min", { m = math.floor((h.planned or 0) / 60 + 0.5) })
    add(line(table.concat(parts, "  ·  ")))
    local a = h.advice or {}
    if a.kind == "hold" then
      add(line(tr("Hold %{secs} s at the next stop", { secs = math.floor(a.secs + 0.5) }), NOW))
    elseif a.kind == "close_behind" then
      add(line(tr("You are close behind %{bus}", { bus = a.bus }), AMBER))
    end
  end
  -- the connections at the stops ahead
  for _, k in ipairs(type(b.connections) == "table" and b.connections or {}) do
    add({ type = "divider" })
    local st = k.status or {}
    local due = k.time .. ((k.late or 0) > 0 and (" (+" .. k.late .. " min)") or "")
    if k.onward then
      add(line(tr("Connection at %{stop}: %{what} %{time}", { stop = k.stop, what = k.what, time = due }), nil))
      if st.kind == "made" then add(line(tr("%{m} min to change", { m = st.m }), GREEN, 12))
      elseif st.kind == "tight" then add(line(tr("Tight: keep the doors short"), AMBER, 12))
      else add(line(tr("Missed at this delay"), RED, 12)) end
    else
      add(line(tr("Connection at %{stop}: wait max %{m} min", { stop = k.stop, m = k.max_hold }), NOW))
      add(line(k.what .. "  " .. due, DIM, 12))
      if st.kind == "wait" then add(line(tr("Hold until %{time}", { time = st["until"] }), NOW, 12))
      elseif st.kind == "too_late" then add(line(tr("Too late to wait for: leave on time"), AMBER, 12))
      else add(line(tr("No wait needed"), GREEN, 12)) end
    end
  end
  -- the diversion ahead, and the day's
  if type(b.ahead) == "table" then
    add({ type = "divider" })
    add(line(tr("Diversion ahead: %{stop} closed", { stop = b.ahead.stop }), NOW))
    if (b.ahead.via or "") ~= "" then add(line(tr("Via %{via}", { via = b.ahead.via }), DIM, 12)) end
  elseif type(b.closed) == "table" and #b.closed > 0 then
    add(line(tr("Closed today: %{stops}", { stops = table.concat(b.closed, ", ") }), DIM, 12))
  end
  return children
end

-- every second (main.lua): the files, the alerts, the card
function M.tick(dt)
  if not (M.on() and fn(omsi.duty, "get")) then return end
  local d = omsi.duty.get()
  if not d then
    if shown and has["ui.remove"] then omsi.ui.remove(PANEL) end
    if shown then to_navigator(nil) end
    shown, brief = false, nil
    return
  end
  write_wait, read_wait = write_wait - (dt or 1), read_wait - (dt or 1)
  if write_wait <= 0 then
    write_wait = WRITE_EVERY
    write_live(d)
  end
  if read_wait <= 0 then
    read_wait = READ_EVERY
    brief = read_brief(d)
    if brief then
      to_navigator(brief)
      for _, a in ipairs(type(brief.alerts) == "table" and brief.alerts or {}) do
        if type(a) == "table" and a.id then alert(a.id, a.priority, a.template or "", a.args) end
      end
    end
  end
  if not has["ui.set"] then return end
  omsi.ui.set(PANEL, { anchor = "top_left", x = 16, y = 330, width = 330, accent = NOW, draggable = true, children = M.rows(d, brief) })
  shown = true
end

-- its close button: off for this drive
function M.click(panel, element)
  if panel ~= PANEL then return false end
  if element == "close" then
    off = true
    shown = false
    if has["ui.remove"] then omsi.ui.remove(PANEL) end
    to_navigator(nil)
  end
  return true
end

function M.stop()
  if has["ui.remove"] then omsi.ui.remove(PANEL) end
end

return M
