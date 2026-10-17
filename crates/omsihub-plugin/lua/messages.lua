-- Omsi-Hub's plugin: the control room's messages to the driver, and his to control - a light
-- CAD/AVL as real operators have it (docs/omsihub/MESSAGES.md).
--
-- Omsi-Hub writes the driver's inbox into the data folder (`data/messages.json`, in the player's
-- language); this module reads it every few seconds and shows what is new as a card with a
-- sound - louder for an urgent message -, and the inbox in a panel of its own: the messages
-- with Accept and Decline for an instruction and the canned replies, and the quick buttons to
-- control (a delay in traffic, a breakdown, ...). What the driver does goes back as one line
-- each (`data/replies.jsonl`); how the duty stands goes back every few seconds
-- (`data/status.json`) for the dispatcher.
--
-- An accepted instruction is carried out in the game, so the route, the IBIS and the navigator
-- follow: a short-turn turns the duty back at the stop it names (the next trip from there), out
-- of service skips the trip's stops to its terminus, a hold counts its minutes down at the next
-- stop, a relief or a spare bus taking over ends the duty where it says.
--
-- The inbox opens by itself while an instruction waits for an answer, and with F9; the panels
-- take the mouse with F10 (main.lua).

local json = require("hubjson")

local M = {}

local PANEL = "omsihub_msgs"
-- (the messages the panel shows, the newest first)
local SHOWN = 5
local COLOR = { info = "#4DA3FF", instruction = "#F0B429", urgent = "#FF5F57" }
local ICON = { info = "chat", instruction = "campaign", urgent = "warning" }
-- the sound of a message (a file of the plugin's folder, Omsi-Hub puts them there) and how loud
local SOUND = { info = { "msg_info.wav", 0.45 }, instruction = { "msg_instruction.wav", 0.75 }, urgent = { "msg_urgent.wav", 1.0 } }
local DIM = "#9AA3AD"

local tr = function(t) return t end
local hook = {}
local inbox = nil
local open = false
local quick = false
local shown = false
local seq = 0
-- (answered here, until Omsi-Hub's inbox says so)
local answered = {}
-- what an accepted instruction still asks of the game, and a hold under way
local todo = {}
local hold = nil
-- the duty taken again by a short-turn: its `duty` event is not a new duty (main.lua)
local turned = nil

omsi.data.msg_seen = omsi.data.msg_seen or {}

local function fn(name)
  local t = omsi
  for part in name:gmatch("[^%.]+") do
    if type(t) ~= "table" then return nil end
    t = t[part]
  end
  return type(t) == "function" and t or nil
end

-- `hooks`: send(file, tag, value, append) and issued() of main.lua, the assignment, and
-- end_trip(how) - the trip under way ended as `how` and reported
function M.setup(translate, hooks)
  tr = translate
  hook = hooks or {}
end

local function clock()
  local i = fn("info") and omsi.info() or nil
  return type(i) == "table" and i.clock or nil
end

local function duty()
  return fn("duty.get") and omsi.duty.get() or nil
end

local function hhmm(t)
  local m = math.floor((t or 0) / 60)
  return string.format("%02d:%02d", (m // 60) % 24, m % 60)
end

-- a card for a while, or a line of text in a game without panels
local function toast(text, title, icon, color, seconds)
  if fn("ui.toast") then
    omsi.ui.toast(text, { title = title, icon = icon, color = color, seconds = seconds or 8 })
  else
    omsi.message((title and title ~= "" and (title .. ": ") or "") .. text, seconds or 8)
  end
end

local function sound(priority)
  local s = SOUND[priority] or SOUND.info
  if fn("audio.play") then omsi.audio.play(s[1], { volume = s[2] }) end
end

-- one line to Omsi-Hub
local function reply(kind, id, key)
  seq = seq + 1
  local a = hook.assignment or {}
  local d = duty()
  local r = {
    kind = kind, id = id or 0, key = key or "", issued = hook.issued and hook.issued() or 0,
    line = (d and d.line) or a.line or "", tour = (d and d.tour) or a.tour or "",
    uid = tostring(hook.issued and hook.issued() or 0) .. "-" .. tostring(math.floor((fn("util.now") and omsi.util.now() or 0))) .. "-" .. seq,
    at = fn("util.now") and omsi.util.now() or 0, clock = clock(),
  }
  if hook.send then hook.send("replies.jsonl", "OMSIHUB-REPLY ", r, true) end
end

local function find(id)
  for _, m in ipairs(inbox and inbox.messages or {}) do
    if m.id == id then return m end
  end
  return nil
end

-- the message waits for the driver's answer
local function waiting(m)
  if m.mine or not m.action or answered[m.id] then return false end
  return m.state ~= "accepted" and m.state ~= "declined"
end

local function any_waiting()
  for _, m in ipairs(inbox and inbox.messages or {}) do
    if waiting(m) then return true end
  end
  return false
end

-- --- carrying an instruction out ---------------------------------------------------------------

-- the stops of the trip under way the bus calls at, with their place in `duty.stops`
local function served(list)
  local out = {}
  for _, s in ipairs(list or {}) do
    if s.stops ~= false then out[#out + 1] = s end
  end
  return out
end

-- the duty's trip (from 1) that leaves at `departure` (seconds of the day), else the one under way
local function trip_at(departure)
  local trips = fn("duty.trips") and omsi.duty.trips() or {}
  for i, t in ipairs(trips) do
    if math.abs((t.departure or -1e9) - (departure or 0)) < 90 then return i end
  end
  local d = duty()
  return d and d.trip or nil
end

-- turn back at stop `stop` (its number) of trip `n`: the next trip from the same stop (the stop
-- of its name, else as far into it as the part skipped)
local function turn_back(t)
  local d = duty()
  if not (d and fn("duty.start")) then return end
  local next_stops = served(fn("duty.trip_stops") and omsi.duty.trip_stops(t.trip + 1) or {})
  if #next_stops == 0 then return end
  local at = nil
  for i, s in ipairs(next_stops) do
    if s.name == t.name then at = i break end
  end
  at = at or math.max(1, math.min(#next_stops, math.floor(#next_stops * t.part + 0.5)))
  if hook.end_trip then hook.end_trip("short_turn") end
  turned = { line = d.line, tour = d.tour }
  local ok = omsi.duty.start(d.line, d.tour, t.trip + 1, at)
  if ok and fn("duty.type_ibis") then omsi.duty.type_ibis() end
  toast(tr("Turned back: the next trip goes on from %{stop}", { stop = next_stops[at].name or "" }), tr("Short-turn"), "u_turn_left", COLOR.instruction)
end

-- what an accepted action asks of the game, now or when the bus gets there
local function carry(a)
  local kind = a.kind
  local n = trip_at(a.departure)
  local d = duty()
  if kind == "short_turn" and n and d then
    local stops = served(n == d.trip and fn("duty.stops") and omsi.duty.stops() or (fn("duty.trip_stops") and omsi.duty.trip_stops(n)) or {})
    if #stops >= 2 then
      local k = math.max(2, math.min(#stops - 1, math.floor(#stops * (1 - (a.part or 0.2)) + 0.5)))
      -- (not a stop the bus passed already)
      for i = k, #stops do
        if not stops[i].passed then k = i break end
      end
      local s = stops[k]
      todo[#todo + 1] = { kind = "turn", trip = n, number = s.number, name = s.name, part = a.part or 0.2 }
      toast(tr("Turn back at %{stop}", { stop = s.name or "" }), tr("Short-turn"), "u_turn_left", COLOR.instruction, 12)
    end
  elseif kind == "out_of_service" and n then
    todo[#todo + 1] = { kind = "express", trip = n }
  elseif kind == "hold" then
    todo[#todo + 1] = { kind = "hold", minutes = a.minutes or 2 }
  elseif kind == "relief" then
    todo[#todo + 1] = { kind = "relief", name = a.stop or "" }
  elseif kind == "swap" and n then
    -- (the spare takes over at the terminus: the duty ends with the trip before)
    todo[#todo + 1] = { kind = "hand_over", trip = n }
  end
end

-- the bus on a trip: what waits for that trip done now
local function on_trip()
  local d = duty()
  if not d then return end
  for i = #todo, 1, -1 do
    local t = todo[i]
    if t.kind == "express" and t.trip == d.trip and fn("duty.skip_to") then
      local stops = fn("duty.stops") and omsi.duty.stops() or {}
      if #stops > 0 then
        omsi.duty.skip_to(#stops)
        toast(tr("Out of service to %{stop}: no more stops on this trip", { stop = stops[#stops].name or "" }), tr("Out of service"), "bolt", COLOR.instruction)
      end
      table.remove(todo, i)
    elseif t.kind == "hand_over" and d.trip >= t.trip then
      table.remove(todo, i)
      if fn("duty.finish") then omsi.duty.finish() end
      toast(tr("A spare bus takes over: take your bus back to the depot"), tr("Spare bus"), "sync_alt", COLOR.instruction, 12)
    end
  end
end

-- the bus at a stop of the trip under way (main.lua's `stop_arrive`)
function M.arrive(number)
  local d = duty()
  if not d then return end
  local stop = d.next_stop or {}
  for i = #todo, 1, -1 do
    local t = todo[i]
    if t.kind == "turn" and t.trip == d.trip and (number or 0) >= (t.number or 0) then
      table.remove(todo, i)
      turn_back(t)
      return
    elseif t.kind == "hold" then
      table.remove(todo, i)
      local now = clock()
      if now then
        hold = { until_ = now + t.minutes * 60 }
        toast(tr("Hold here for %{m} min", { m = t.minutes }), tr("Hold"), "timer", COLOR.instruction)
      end
    elseif t.kind == "relief" and (t.name == "" or t.name == stop.name or t.name == (d.previous_stop or {}).name) then
      table.remove(todo, i)
      if hook.end_trip then hook.end_trip("relieved") end
      if fn("duty.finish") then omsi.duty.finish() end
      toast(tr("Your relief takes over here: your duty ends"), tr("Relief"), "group", COLOR.instruction, 12)
    end
  end
end

-- a duty event of the turn just made (not a new duty: main.lua says nothing of it)
function M.turned(line, tour)
  if turned and line == turned.line and tour == turned.tour then
    turned = nil
    return true
  end
  return false
end

-- --- the inbox ----------------------------------------------------------------------------------

local function choices(list, prefix)
  local out = {}
  for _, c in ipairs(list or {}) do
    out[#out + 1] = { type = "button", id = prefix .. c.key, text = c.label or c.key, icon = c.icon }
  end
  return out
end

local function children()
  local list = inbox and inbox.messages or {}
  local out = {
    { type = "row", align = "between", children = {
      { type = "icon", name = "forum", color = "#F47F30" },
      { type = "text", text = tr("Control"), weight = "bold", grow = true, wrap = false },
      { type = "button", id = "msg_close", text = "×" },
    } },
  }
  if hold then
    local left = math.max(0, hold.until_ - (clock() or hold.until_))
    out[#out + 1] = { type = "badge", text = tr("Hold: %{time} left", { time = string.format("%d:%02d", math.floor(left / 60), math.floor(left % 60)) }), color = COLOR.instruction, text_color = "#1A1400" }
  end
  if #list == 0 then
    out[#out + 1] = { type = "text", text = tr("No messages"), size = 13, color = DIM }
  end
  for k = 1, math.min(SHOWN, #list) do
    local m = list[k]
    out[#out + 1] = { type = "divider" }
    out[#out + 1] = { type = "row", children = {
      { type = "icon", name = m.mine and "near_me" or (ICON[m.priority] or "chat"), size = 16, color = COLOR[m.priority] or DIM },
      { type = "text", text = (m.time or "") .. "  ·  " .. (m.from or ""), size = 12, color = DIM, grow = true, wrap = false },
      { type = "text", text = answered[m.id] and tr(answered[m.id]) or (m.state_label or ""), size = 12, color = DIM, wrap = false },
    } }
    out[#out + 1] = { type = "text", text = m.text or "", size = 14, weight = m.priority == "urgent" and "bold" or "regular" }
    if m.reply and m.reply ~= "" then out[#out + 1] = { type = "text", text = "↳ " .. m.reply, size = 12, color = DIM } end
    if m.outcome and m.outcome ~= "" then out[#out + 1] = { type = "text", text = m.outcome, size = 12, color = COLOR.urgent } end
    if waiting(m) then
      out[#out + 1] = { type = "row", children = {
        { type = "button", id = "msg_accept_" .. m.id, text = tr("Accept"), icon = "check" },
        { type = "button", id = "msg_decline_" .. m.id, text = tr("Decline"), icon = "close" },
      } }
    elseif not m.mine and not m.action and m.state ~= "read" and not answered[m.id] and k <= 2 then
      out[#out + 1] = { type = "row", children = choices(inbox.replies, "msg_reply_" .. m.id .. "_") }
    end
  end
  out[#out + 1] = { type = "divider" }
  out[#out + 1] = { type = "button", id = "msg_quick", text = quick and tr("Hide") or tr("Report to control"), icon = quick and "expand_less" or "campaign" }
  if quick then
    local q = choices(inbox and inbox.quick or {}, "msg_q_")
    for i = 1, #q, 2 do
      out[#out + 1] = { type = "row", children = { q[i], q[i + 1] } }
    end
  end
  out[#out + 1] = { type = "text", text = tr("F9: messages · F10: use the panels with the mouse"), size = 11, color = DIM }
  return out
end

local function panel()
  if not fn("ui.set") then return end
  local want = (open or any_waiting() or hold ~= nil) and inbox ~= nil
  if not want then
    if shown and fn("ui.remove") then omsi.ui.remove(PANEL) end
    shown = false
    return
  end
  omsi.ui.set(PANEL, { anchor = "bottom_right", x = 16, y = 96, width = 340, accent = "#F47F30", draggable = true, children = children() })
  shown = true
end

-- the inbox read again (every few seconds): what is new told
function M.read()
  if not (fn("files.read") and fn("json.decode")) then return end
  local text = omsi.files.read("messages.json")
  if not text then return end
  local ok, v = pcall(omsi.json.decode, text)
  if not ok or type(v) ~= "table" or type(v.messages) ~= "table" then return end
  inbox = v
  -- (the oldest first, so the newest card is on top)
  for k = #v.messages, 1, -1 do
    local m = v.messages[k]
    if type(m) == "table" and m.id and not m.mine and not omsi.data.msg_seen[m.id] then
      omsi.data.msg_seen[m.id] = true
      if m.state == "sent" then reply("delivered", m.id) end
      toast(m.text or "", m.from, ICON[m.priority] or "chat", COLOR[m.priority], m.priority == "urgent" and 20 or 10)
      sound(m.priority)
      if m.action then open = true end
    end
  end
  -- (what Omsi-Hub's inbox says now wins)
  for id in pairs(answered) do
    local m = find(id)
    if not m or m.state == "accepted" or m.state == "declined" or m.state == "read" then answered[id] = nil end
  end
  panel()
end

-- how the duty stands, for the dispatcher (every few seconds)
function M.status()
  local d = duty()
  if not (d and fn("files.write")) then return end
  local trips = fn("duty.trips") and omsi.duty.trips() or {}
  local this, after = trips[d.trip] or {}, trips[d.trip + 1] or {}
  local stops = served(fn("duty.stops") and omsi.duty.stops() or {})
  local passed = 0
  for _, s in ipairs(stops) do
    if s.passed then passed = passed + 1 end
  end
  local st = {
    issued = hook.issued and hook.issued() or 0, line = d.line or "", tour = d.tour or "",
    trip = d.trip or 0, trips = #trips, departure = this.departure or d.departure, arrival = this.arrival or d.arrival,
    next_departure = after.departure, next_arrival = after.arrival, delay = d.delay,
    stops = #stops, passed = passed, trip_done = d.trip_done == true,
    at = fn("util.now") and omsi.util.now() or 0, clock = clock(),
  }
  omsi.files.write("status.json", json.encode(st))
end

-- every second: the hold's countdown, what waits for the trip under way
function M.tick()
  on_trip()
  if hold then
    local now = clock()
    if now and now >= hold.until_ then
      hold = nil
      toast(tr("Hold over: carry on"), tr("Hold"), "play_arrow", COLOR.info)
    end
  end
  if hold or shown then panel() end
end

-- a click of the inbox; whether it was this module's
function M.click(p, element)
  if p ~= PANEL then return false end
  local e = element or ""
  if e == "msg_close" then
    open, quick = false, false
  elseif e == "msg_quick" then
    quick = not quick
  elseif e:sub(1, 6) == "msg_q_" then
    local key = e:sub(7)
    reply("quick", 0, key)
    quick = false
    toast(tr("Sent to control"), tr("Control"), "campaign", COLOR.info, 5)
  else
    local verb, id = e:match("^msg_(%a+)_(%d+)")
    id = tonumber(id)
    local m = id and find(id)
    if m and verb == "accept" and waiting(m) then
      reply("accept", id)
      answered[id] = "Accepted"
      carry(m.action)
    elseif m and verb == "decline" and waiting(m) then
      reply("decline", id)
      answered[id] = "Declined"
    elseif m and verb == "reply" then
      local key = e:match("^msg_reply_%d+_(.+)$")
      reply("reply", id, key)
      answered[id] = "Read"
    end
  end
  panel()
  return true
end

-- F9: the inbox shown or put away; whether it was this module's
function M.key(key, down)
  if key == "F9" and down and inbox then
    open = not open
    panel()
    return true
  end
  return false
end

return M
