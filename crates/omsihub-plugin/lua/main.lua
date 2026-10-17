-- Omsi-Hub in openOMSI: the duty the bus company assigned, its trips reported back to the
-- company, and the company's notices on the screen - through openOMSI's own plugin API, so
-- that Omsi-Hub needs nothing of the game's code.
--
-- Installed and kept up to date by Omsi-Hub (crates/omsihub-plugin); a change by hand goes
-- with its next update. The files it shares with Omsi-Hub (see src/bridge.rs):
--   assignment.lua        Omsi-Hub -> plugin: the duty, the notices, the texts
--   data/notices.json     Omsi-Hub -> plugin: notices that come while the game runs
--   data/messages.json    Omsi-Hub -> plugin: the control room's messages (messages.lua)
--   data/hello.json       plugin -> Omsi-Hub: the handshake (what this openOMSI offers)
--   data/reports.jsonl    plugin -> Omsi-Hub: the duty taken, each trip done, the duty ended
--   data/replies.jsonl    plugin -> Omsi-Hub: the driver's answers and quick buttons
--   data/status.json      plugin -> Omsi-Hub: how the duty stands (for the dispatcher)
-- A game without the file functions gets the handshake and the reports in its log instead.

local VERSION = "1.6.0"
local BRIDGE = 1
local PANEL = "omsihub"

local json = require("hubjson")

-- The functions the add-on uses (src/compat.rs FEATURES; a test keeps the two lists the same),
-- looked up in the `omsi` table itself so that this works in an openOMSI without game.has.
local NEEDS = {
  "duty.get", "duty.trips", "files.append", "files.write", "duty.start",
  "ui.set", "ui.toast", "ui.remove", "files.read", "json.decode", "send",
  "duty.set_auto_ibis", "duty.type_ibis", "bus.set_display_font", "game.action",
  "duty.skip_to", "duty.finish", "duty.trip_stops", "audio.play",
  "duty.set_closed_stops",
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

-- the assignment Omsi-Hub wrote beside this file (none: the player drives on their own); one
-- older than half a day is of a game Omsi-Hub started before, not of this one
local assignment
do
  local ok, a = pcall(require, "assignment")
  if ok and type(a) == "table" then assignment = a end
  local now = fn("util.now") and omsi.util.now() or 0
  if assignment and now > 0 and (tonumber(assignment.issued) or 0) > 0 and now - assignment.issued > 12 * 3600 then assignment = nil end
end

-- the plugin's texts in the player's language: Omsi-Hub writes them beside the plugin
-- (`texts.lua`), and an older Omsi-Hub put them into the assignment
local texts = {}
do
  local ok, t = pcall(require, "texts")
  if ok and type(t) == "table" then texts = t end
end
local old_texts = assignment and type(assignment.texts) == "table" and assignment.texts or {}

-- a text in the player's language, with %{name} filled from `vars`
local function tr(text, vars)
  local t = texts[text] or old_texts[text] or text
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
  return done
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
  return send_out("reports.jsonl", "OMSIHUB-REPORT ", r, true)
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

-- (a company's duty: a drive of the player's own has an assignment only for its driver)
local function company_duty()
  return assignment and (assignment.duty or "") ~= ""
end

local signon = require("signon")
local board = require("board")
local company = require("company")
board.setup(tr)

-- the board's company parts the driver switched on and off (all on until they chose; kept for
-- the next drive)
omsi.data.board_parts = omsi.data.board_parts or {}
local function parts()
  return company.parts(omsi.data.board_parts)
end
if company_duty() then signon.setup(tr, assignment) end
-- (the on-time card: punctuality, headway, connections, diversions - `ontime.lua`, its own)
local ontime = require("ontime")
ontime.setup(tr, assignment, has)

-- the duty strip, the handover notes and the walk-around of a company's duty (duty_strip.lua):
-- its notes and defects go to Omsi-Hub as reports, its notices as cards
local strip = require("duty_strip")
strip.setup(tr, company_duty() and assignment.crew or nil, function(kind, fields) report(kind, fields) end, function(icon, title, text)
  if has["ui.toast"] then omsi.ui.toast(text, { title = title, icon = icon, seconds = 8 }) else omsi.message(title .. ": " .. text, 8) end
end)

local function panel()
  if not (company_duty() and has["ui.set"]) then return end
  local d = duty_now()
  local status
  if d then
    status = tr("On duty: trip %{trip} of %{trips}", { trip = d.trip, trips = d.trips })
  else
    status = tr("Not started yet")
  end
  local work = signon.at_work()
  local can_start = work and not d and has["duty.start"] and (assignment.line or "") ~= ""
  local children = {
    { type = "row", children = {
      { type = "icon", name = "business", color = "#F58620" },
      { type = "text", text = assignment.company ~= "" and assignment.company or "Omsi-Hub", weight = "bold", grow = true, wrap = false },
    } },
    { type = "text", text = tr("Duty %{duty}: line %{line}, tour %{tour}", { duty = assignment.duty, line = assignment.line, tour = assignment.tour }) },
  }
  if work then
    children[#children + 1] = { type = "text", id = "status", text = status, size = 13 }
    children[#children + 1] = { type = "button", id = "start", text = tr("Start duty"), icon = "play_arrow", visible = can_start }
    children[#children + 1] = { type = "button", id = "signoff", text = tr("Sign off"), icon = "logout", visible = signon.signed() }
  else
    for _, c in ipairs(signon.children(assignment)) do children[#children + 1] = c end
  end
  children[#children + 1] = { type = "text", text = tr("F10: use the panel with the mouse"), size = 12, color = "#9AA3AD", visible = can_start or not work }
  -- (while the panels have the mouse: the board's company parts, to switch on and off)
  if work and fn("ui.focused") and omsi.ui.focused() then
    local on = parts()
    children[#children + 1] = { type = "divider" }
    children[#children + 1] = { type = "text", text = tr("On the duty board"), size = 12, weight = "bold", color = "#9AA3AD" }
    for _, p in ipairs(company.PARTS) do
      children[#children + 1] = { type = "checkbox", id = "part_" .. p, text = tr(company.LABELS[p]), checked = on[p], color = "#F58620" }
    end
  end
  omsi.ui.set(PANEL, { anchor = "right", x = 16, y = 0, width = 300, accent = "#F58620", draggable = true, children = children })
end

-- the IBIS: typed by the game as each trip begins, unless Omsi-Hub says the driver types it -
-- and not before the driver accepted the duty order when signing on
local function ibis_rule()
  if not has["duty.set_auto_ibis"] then return end
  omsi.duty.set_auto_ibis(signon.at_work() and not (assignment and assignment.ibis == false))
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

-- what the duty's timetable says of trip `n` (from 1), and the bus and the map it is driven on
local function trip_facts(n)
  local f = { trip = n, trip_name = "", trips = 0, number = "", terminus = "", bus = "", map = "" }
  if has["duty.trips"] then
    local all = omsi.duty.trips()
    local t = all[n]
    f.trips = #all
    if t then
      f.trip_name, f.number, f.terminus = t.name or "", t.line or "", t.terminus or ""
      f.departure, f.arrival = t.departure, t.arrival
    end
  end
  if fn("bus.file") then f.bus = omsi.bus.file() or "" end
  if fn("map.info") then
    local m = omsi.map.info()
    if type(m) == "table" and type(m.path) == "string" then f.map = m.path end
  end
  return f
end

local function trip_report(line, tour, n, fields)
  local r = trip_facts(n)
  r.line, r.tour = line or "", tour or ""
  for k, v in pairs(fields) do r[k] = v end
  return report("trip", r)
end

-- --- the trip's card and the driver's record -----------------------------------------------

-- (an openOMSI with game.has also has trip_done; an older one is watched trip by trip)
local trip_events = fn("game.has") ~= nil

local trip = require("trip")
local card = require("card")
local watch = require("watch")
card.setup(tr)

-- the drive watch's notices: a card for a while, or a line of text in a game without panels
watch.setup(tr, function(icon, title, text)
  if has["ui.toast"] then
    omsi.ui.toast(text, { title = title, icon = icon, seconds = 5, color = icon == "traffic" and "#D93A30" or "#E8AA46" })
  else
    omsi.message(title .. ": " .. text, 5)
  end
end)
if fn("game.settings") then
  local s = omsi.game.settings()
  -- (the units the player reads; an openOMSI without them says the API's own)
  if type(s) == "table" then watch.units = s.display_units or s.units end
end

local function standing()
  return not omsi.speed or omsi.speed() < 1
end

-- a trip that ended: its record into the report (Omsi-Hub books it), its card on the screen
local function trip_ended(run, fields)
  local c = trip.current()
  fields = fields or {}
  if run then fields.record = run end
  local saved = trip_report(state.line or (c and c.line_name), state.tour or (c and c.tour), fields.trip or (c and c.n), fields)
  if run then card.hold(run, saved, standing()) end
end

-- the trip under way, looked at a few times a second: another one began
local look = 0
local function follow_trip(dt)
  look = look - dt
  if look > 0 then return end
  look = 0.25
  local d = duty_now()
  if not d then
    trip.forget()
    return
  end
  local c = trip.current()
  if c and c.n == d.trip and c.line_name == d.line then return end
  -- (an older openOMSI without `trip_done`: `watch_trips` ends the trip and begins the next)
  if c and not c.done and not trip_events then return end
  trip.begin(d.trip, d.line, d.tour)
end

local function delay()
  local d = duty_now()
  return d and d.delay or nil
end


-- the control room's messages (messages.lua): the trip a short-turn or a relief ends reported
local messages = require("messages")
messages.setup(tr, { send = send_out, issued = issued, assignment = assignment, end_trip = function(how)
  local d = duty_now()
  if not d then return end
  local c = trip.current()
  local run = c and c.n == d.trip and trip.finish(false, delay(), watch) or nil
  trip_ended(run, { trip = d.trip, how = how, delay = delay() })
end })

omsi.on("duty", function(line, tour)
  -- (a short-turn takes the same duty again at its next trip: no new duty)
  if messages.turned(line, tour) then return panel() end
  if state.line then report("duty_end", { line = state.line, tour = state.tour }) end
  -- (the company's duty ended: the walk-around before the driver leaves the bus)
  if state.line and not line then strip.sign_off() end
  state.line, state.tour, state.last_trip = line, tour, nil
  if line then report("duty_start", { line = line, tour = tour }) end
  panel()
end)

omsi.on("trip_done", function(n, how, driving, comfort, tickets)
  local fields = { trip = n, how = how, driving = driving, comfort = comfort, tickets = tickets, delay = delay() }
  local c = trip.current()
  local run = c and c.n == n and trip.finish(how == "arrived", delay(), watch) or nil
  trip_ended(run, fields)
  panel()
end)

omsi.on("stop_arrive", function(_, _, number, d)
  trip.arrive(number, d)
  messages.arrive(number)
end)
omsi.on("stop_depart", function(_, _, number, d) trip.depart(number, d) end)
omsi.on("jolt", function() trip.jolt() end)
omsi.on("crash", function() trip.crash() end)
omsi.on("passenger_board", function(count) trip.boarded(count) end)
omsi.on("light_ahead", function(aspect) watch.light_ahead(aspect) end)
omsi.on("red_light", function(kmh) watch.red_light(kmh) end)

-- (the object editor once, as soon as the world is there: the frames come with it)
local editor_opened = false

omsi.on("frame", function(dt)
  if assignment and assignment.editor and not editor_opened then
    editor_opened = true
    if has["game.action"] then omsi.game.action("object_editor") end
  end
  if fn("has_vehicle") and not omsi.has_vehicle() then return end
  trip.frame(dt)
  watch.frame(dt)
  strip.frame(dt)
  follow_trip(dt)
  card.frame(dt, standing())
end)

local function watch_trips()
  if trip_events then return end
  local d = duty_now()
  if not d then return end
  if state.last_trip and d.trip > state.last_trip then
    local c = trip.current()
    local run = c and c.n == state.last_trip and trip.moved_on(watch) or nil
    trip_ended(run, { trip = state.last_trip, how = "arrived", delay = d.delay })
    trip.begin(d.trip, d.line, d.tour)
  end
  state.last_trip = d.trip
end

-- the display font chosen for the drive's bus, on that bus
local function display_font()
  local font = assignment and assignment.display_font or ""
  if font == "" or not has["bus.set_display_font"] then return end
  local file = fn("bus.file") and omsi.bus.file() or ""
  local want = (assignment.bus or ""):gsub("\\", "/"):lower()
  if want ~= "" and file:gsub("\\", "/"):lower() ~= want then return end
  omsi.bus.set_display_font(font)
end

omsi.on("vehicle", function(name)
  if name then display_font() end
  -- (the notices once the player is in the bus and sees them)
  if name and assignment then show_notices(assignment.notices) end
  if name and signon.at_work() then strip.signed_on() end
  if name then ibis_rule() end
  if name and assignment and assignment.auto_start and signon.at_work() and not state.started then
    state.started = true
    start_duty()
  end
  panel()
end)

omsi.on("ui_click", function(p, element)
  if card.click(p, element) then return end
  if messages.click(p, element) then return end
  if strip.click(p, element) then return end
  if p == PANEL and element == "signoff" and signon.signed() then strip.sign_off() end
  if ontime.click(p, element) then return end
  if p == PANEL and element == "start" then start_duty() end
  -- (signed on and the duty accepted: taken now where it was to start by itself)
  if p == PANEL and signon.click(element, function()
    if assignment.auto_start and not state.started then
      state.started = true
      start_duty()
    end
    -- (the duty's codes go to the IBIS as the order is accepted; the bus's notes are shown)
    strip.signed_on()
    ibis_rule()
    if has["duty.type_ibis"] and assignment.ibis ~= false then omsi.duty.type_ibis() end
  end) then
    ibis_rule()
    panel()
  end
end)

omsi.on("ui_change", function(p, element, value)
  if card.change(p, element, value) then return end
  if strip.change(p, element, value) then return end
  local part = p == PANEL and type(element) == "string" and element:match("^part_(%w+)$")
  if part and company.LABELS[part] then
    omsi.data.board_parts[part] = value == true
    -- (the duty strip is duty_strip.lua's, the only one: its switch is the board's "strip" part)
    if part == "strip" then strip.set_enabled(value == true) end
    panel()
    return
  end
  if p == PANEL and signon.change(element, value) then panel() end
end)

-- (Ctrl, Alt and Shift held: a key with them is another one's - Ctrl+Enter the IBIS's)
local modifiers = {}
omsi.on("key", function(key, down)
  if type(key) == "string" and (key:match("^Control") or key:match("^Alt") or key:match("^Shift")) then modifiers[key] = down or nil end
  if card.key(key, down, next(modifiers) ~= nil) then return end
  if messages.key(key, down) then return end
  if key == "F10" and down and (company_duty() or duty_now()) and fn("ui.focus") then
    omsi.ui.focus(not omsi.ui.focused())
    panel()
  end
end)

omsi.on("stop", function()
  if has["ui.remove"] then omsi.ui.remove(PANEL) end
  strip.stop()
  ontime.stop()
end)

-- service control's messages for the duty (`company.lua`), each once, when they are due
omsi.data.dispatched = omsi.data.dispatched or {}
local function dispatch()
  if not (company_duty() and parts().dispatch and signon.at_work() and duty_now()) then return end
  local c = clock()
  if not c then return end
  -- (said once for this assignment: a drive started again does not say them twice)
  local seen = omsi.data.dispatched
  if seen.issued ~= issued() then
    seen = { issued = issued() }
    omsi.data.dispatched = seen
  end
  for _, m in ipairs(company.messages(assignment, math.floor(c / 60), seen)) do
    seen[m.id] = true
    if has["ui.toast"] then
      omsi.ui.toast(m.text, { title = m.title, icon = "campaign", color = "#F58620", seconds = 12 })
    else
      omsi.message(m.title .. ": " .. m.text, 10)
    end
  end
end

-- (the duty board: wanted unless Omsi-Hub says not, and not before the driver signed on)
omsi.every(1, function()
  board.update((not assignment or assignment.board ~= false) and signon.at_work(), company_duty() and parts() or {}, assignment, { trip.boarded_on_trip(), trip.boarded_total(), fn("bus.passengers") and omsi.bus.passengers() or nil })
  messages.tick()
  strip.update(company_duty() and signon.at_work())
  if signon.at_work() then ontime.tick(1) end
  dispatch()
end)

omsi.every(2, function()
  watch_trips()
  panel()
end)
omsi.every(5, live_notices)
omsi.every(2, messages.read)
omsi.every(5, messages.status)

hello()
panel()
