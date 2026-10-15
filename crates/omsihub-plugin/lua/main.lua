-- Omsi-Hub in openOMSI: the duty the bus company assigned, its trips reported back to the
-- company, and the company's notices on the screen - through openOMSI's own plugin API, so
-- that Omsi-Hub needs nothing of the game's code.
--
-- Installed and kept up to date by Omsi-Hub (crates/omsihub-plugin); a change by hand goes
-- with its next update. The files it shares with Omsi-Hub (see src/bridge.rs):
--   assignment.lua        Omsi-Hub -> plugin: the duty, the notices, the texts
--   data/notices.json     Omsi-Hub -> plugin: notices that come while the game runs
--   data/hello.json       plugin -> Omsi-Hub: the handshake (what this openOMSI offers)
--   data/reports.jsonl    plugin -> Omsi-Hub: the duty taken, each trip done, the duty ended
-- A game without the file functions gets the handshake and the reports in its log instead.

local VERSION = "1.0.0"
local BRIDGE = 1
local PANEL = "omsihub"

local json = require("hubjson")

-- The functions the add-on uses (src/compat.rs FEATURES; a test keeps the two lists the same),
-- looked up in the `omsi` table itself so that this works in an openOMSI without game.has.
local NEEDS = {
  "duty.get", "duty.trips", "files.append", "files.write", "duty.start",
  "ui.set", "ui.toast", "ui.remove", "files.read", "json.decode", "send",
}

local function fn(name)
  local t = omsi
  for part in name:gmatch("[^%.]+") do
    if type(t) ~= "table" then return nil end
    t = t[part]
  end
  if type(t) == "function" then return t end
  return nil
end

local has = {}
for _, n in ipairs(NEEDS) do has[n] = fn(n) ~= nil end

-- the assignment Omsi-Hub wrote beside this file (none: the player drives on their own)
local assignment
do
  local ok, a = pcall(require, "assignment")
  if ok and type(a) == "table" then assignment = a end
end

-- a text in the player's language (Omsi-Hub puts them into the assignment), with %{name}
-- filled from `vars`
local function tr(text, vars)
  local t = assignment and type(assignment.texts) == "table" and assignment.texts[text] or text
  if vars then
    t = t:gsub("%%{(%w+)}", function(k)
      local v = vars[k]
      if v == nil then return "" end
      return tostring(v)
    end)
  end
  return t
end

local function issued()
  return assignment and tonumber(assignment.issued) or 0
end

-- one line to Omsi-Hub: into a file of the data folder, else into the game's log
local function send_out(file, tag, value, append)
  local text = json.encode(value)
  local done = false
  if append and has["files.append"] then
    done = omsi.files.append(file, text .. "\n") == true
  elseif not append and has["files.write"] then
    done = omsi.files.write(file, text) == true
  end
  if not done then omsi.log(tag .. text) end
  local port = assignment and tonumber(assignment.udp_port) or 0
  if append and port > 0 and has["send"] then omsi.send(port, text) end
end

local function hello()
  local h = {
    bridge = BRIDGE, plugin = VERSION, game = "", api = 1, platform = "", language = "",
    functions = has, issued = issued(),
  }
  if fn("game.version") then h.game = omsi.game.version() end
  if fn("game.api") then h.api = omsi.game.api() end
  if fn("game.platform") then h.platform = omsi.game.platform() end
  if fn("game.settings") then
    local s = omsi.game.settings()
    if type(s) == "table" and type(s.language) == "string" then h.language = s.language end
  end
  send_out("hello.json", "OMSIHUB-HELLO ", h, false)
end

local function now()
  if fn("util.now") then return omsi.util.now() end
  return 0
end

local function clock()
  if not fn("info") then return nil end
  local i = omsi.info()
  return type(i) == "table" and i.clock or nil
end

local function duty_now()
  if not has["duty.get"] then return nil end
  return omsi.duty.get()
end

local function report(kind, fields)
  local r = {
    kind = kind, issued = issued(), duty = assignment and assignment.duty or "",
    at = now(), clock = clock(),
  }
  for k, v in pairs(fields) do r[k] = v end
  send_out("reports.jsonl", "OMSIHUB-REPORT ", r, true)
end

-- --- notices ----------------------------------------------------------------------------

omsi.data.seen = omsi.data.seen or {}

local function show_notice(n)
  if type(n) ~= "table" or not n.id or omsi.data.seen[n.id] then return end
  omsi.data.seen[n.id] = true
  local text = n.text or ""
  if has["ui.toast"] then
    omsi.ui.toast(text, { title = n.title, icon = n.kind == "alert" and "warning" or "business", seconds = 12 })
  else
    local title = n.title and n.title ~= "" and (n.title .. ": ") or ""
    omsi.message(title .. text, 10)
  end
end

local function show_notices(list)
  if type(list) ~= "table" then return end
  for _, n in ipairs(list) do show_notice(n) end
end

local function live_notices()
  if not (has["files.read"] and has["json.decode"]) then return end
  local text = omsi.files.read("notices.json")
  if not text then return end
  show_notices(omsi.json.decode(text))
end

-- --- the panel ----------------------------------------------------------------------------

local state = { line = nil, tour = nil, last_trip = nil, started = false }

local function panel()
  if not (assignment and has["ui.set"]) then return end
  local d = duty_now()
  local status
  if d then
    status = tr("On duty: trip %{trip} of %{trips}", { trip = d.trip, trips = d.trips })
  else
    status = tr("Not started yet")
  end
  local can_start = not d and has["duty.start"] and (assignment.line or "") ~= ""
  omsi.ui.set(PANEL, {
    anchor = "right", x = 16, y = 0, width = 300, accent = "#F47F30", draggable = true,
    children = {
      { type = "row", children = {
        { type = "icon", name = "business", color = "#F47F30" },
        { type = "text", text = assignment.company ~= "" and assignment.company or "Omsi-Hub", weight = "bold", grow = true, wrap = false },
      } },
      { type = "text", text = tr("Duty %{duty}: line %{line}, tour %{tour}", { duty = assignment.duty, line = assignment.line, tour = assignment.tour }) },
      { type = "text", id = "status", text = status, size = 13 },
      { type = "button", id = "start", text = tr("Start duty"), icon = "play_arrow", visible = can_start },
      { type = "text", text = tr("F10: use the panel with the mouse"), size = 12, color = "#9AA3AD", visible = can_start },
    },
  })
end

local function start_duty()
  if not (assignment and has["duty.start"]) or duty_now() then return end
  local ok, why = omsi.duty.start(assignment.line, assignment.tour, tonumber(assignment.trip) or 1, tonumber(assignment.stop) or 1)
  if not ok then
    local text = tr("The duty could not be started: %{reason}", { reason = why or "" })
    if has["ui.toast"] then omsi.ui.toast(text, { icon = "warning" }) else omsi.message(text, 8) end
  end
  panel()
end

-- --- the duty and its trips -------------------------------------------------------------------

local function trip_name(n)
  if not has["duty.trips"] then return "" end
  local t = omsi.duty.trips()[n]
  return t and t.name or ""
end

local function delay()
  local d = duty_now()
  return d and d.delay or nil
end

-- (an openOMSI with game.has also has trip_done; an older one is watched trip by trip)
local trip_events = fn("game.has") ~= nil

omsi.on("duty", function(line, tour)
  if state.line then report("duty_end", { line = state.line, tour = state.tour }) end
  state.line, state.tour, state.last_trip = line, tour, nil
  if line then report("duty_start", { line = line, tour = tour }) end
  panel()
end)

omsi.on("trip_done", function(trip, how, driving, comfort, tickets)
  report("trip", {
    line = state.line or "", tour = state.tour or "", trip = trip, trip_name = trip_name(trip),
    how = how, driving = driving, comfort = comfort, tickets = tickets, delay = delay(),
  })
  panel()
end)

local function watch_trips()
  if trip_events then return end
  local d = duty_now()
  if not d then return end
  if state.last_trip and d.trip > state.last_trip then
    report("trip", { line = d.line, tour = d.tour, trip = state.last_trip, trip_name = trip_name(state.last_trip), how = "arrived", delay = d.delay })
  end
  state.last_trip = d.trip
end

omsi.on("vehicle", function(name)
  -- (the notices once the player is in the bus and sees them)
  if name and assignment then show_notices(assignment.notices) end
  if name and assignment and assignment.auto_start and not state.started then
    state.started = true
    start_duty()
  end
  panel()
end)

omsi.on("ui_click", function(p, element)
  if p == PANEL and element == "start" then start_duty() end
end)

omsi.on("key", function(key, down)
  if key == "F10" and down and assignment and fn("ui.focus") then omsi.ui.focus(not omsi.ui.focused()) end
end)

omsi.on("stop", function()
  if has["ui.remove"] then omsi.ui.remove(PANEL) end
end)

omsi.every(2, function()
  watch_trips()
  panel()
end)
omsi.every(5, live_notices)

hello()
panel()
