-- Omsi-Hub's plugin: the bus company on the duty board while driving - what the company knows of
-- the duty besides its timetable, from the assignment Omsi-Hub wrote (the company's duty sheet,
-- service control's decisions, the fares, the line's contract) and from the duty as it runs.
-- Five parts, each switched on and off by the driver (the company's panel, F10) and kept for the
-- next drive:
--
-- * coach     - punctuality coaching: at a stop too early, how long to hold and until when;
--               running late, whether the layover at the terminus brings the next trip back
--               on time;
-- * strip     - the duty strip: the next break or relief, and where and when the duty ends;
-- * dispatch  - service control's messages: its decisions for the tour (a short-turn, a trip
--               out of service, a layover cut, another bus) and the reliefs and breaks coming,
--               each once, a few minutes before;
-- * fares     - the fare box: the passengers boarded on this trip and on the duty, what they
--               bring the company (none where the fares go to the authority), and the load
--               aboard now;
-- * contract  - the line's contract with the authority: how it rates the company, and the
--               on-time window it counts.
--
-- Everything here is worked out by pure functions from the duty (`duty.get`), its trips, the
-- clock and the assignment, so the tests check it without a game.

local career = require("career")

local M = {}

M.PARTS = { "coach", "strip", "dispatch", "fares", "contract" }
M.LABELS = {
  coach = "Punctuality coach", strip = "Duty strip", dispatch = "Service control messages",
  fares = "Fare box", contract = "Contract",
}

local ORANGE, AMBER, GREEN, RED, BLUE, DIM, SOFT = "#F58620", "#E8AA46", "#37D67A", "#FF5F57", "#4DA3FF", "#959DB0", "#C4CAD6"
-- (a message comes this long before what it is about)
local AHEAD = 5 * 60
-- (a stop's hold under this is not worth a word)
local HOLD_MIN = 15
-- (the least layover a bus stands at the terminus, `company::control::MIN_LAYOVER`)
local MIN_LAYOVER = 3 * 60

local tr = function(t) return t end

function M.setup(translate)
  tr = translate
end

local function hhmm(t)
  local m = math.floor((t or 0) / 60)
  return string.format("%02d:%02d", (m // 60) % 24, m % 60)
end

-- minutes of the day as hh:mm
local function hm(minutes)
  return hhmm((minutes or 0) * 60)
end

local function mmss(s)
  s = career.round(math.abs(s))
  return string.format("%d:%02d", s // 60, s % 60)
end

local function offset(d)
  return (d < 0 and "−" or "+") .. mmss(d)
end

-- the parts switched on: the driver's choice, every part on until chosen
function M.parts(saved)
  local on = {}
  for _, p in ipairs(M.PARTS) do on[p] = not (type(saved) == "table" and saved[p] == false) end
  return on
end

-- --- the punctuality coach -------------------------------------------------------------------

-- the coach's advice for the duty `d` (`duty.get`), its trips and the clock (s), the line's
-- contract (or none): a box's text, its colour and icon, or nil when there is nothing to say
function M.coach(d, trips, clock, contract)
  if not d or d.trip_done then return nil end
  local delay = d.delay or 0
  local s = d.next_stop
  local first = s and s.number == 1
  -- standing early: hold until the stop's departure (before the first stop that is waiting
  -- for the trip to begin, no "early")
  if d.at_stop and s and not first and s.departure and delay < 0 then
    local hold = s.departure - clock
    if hold >= HOLD_MIN then
      -- (what leaving now would cost: the contract's window, else OMSI 2's)
      local window = contract and (contract.early or 1) * 60 or -career.EARLY_DEPARTURE
      local costly = hold > window
      return {
        text = tr("Hold %{s} here · leave at %{time}", { s = mmss(hold), time = hhmm(s.departure) }),
        note = costly and tr("Leaving now counts as early") or nil,
        color = costly and RED or BLUE, icon = "pan_tool",
      }
    end
    return nil
  end
  if not d.at_stop and not first and delay < -60 then
    return { text = tr("Running %{s} early · ease off", { s = mmss(delay) }), color = BLUE, icon = "speed" }
  end
  -- late: what the layover at the terminus gives back for the next trip
  if delay > career.LATE_ARRIVAL then
    local this, after = trips[d.trip], trips[d.trip + 1]
    if not (this and after) then
      return { text = tr("Late %{s} · last trip of the duty", { s = offset(delay) }), color = RED, icon = "schedule" }
    end
    local layover = (after.departure or 0) - (this.arrival or 0)
    local left = delay - math.max(layover - MIN_LAYOVER, 0)
    local m = math.max(math.floor(layover / 60), 0)
    if left <= 60 then
      return { text = tr("Late %{s} · the %{m} min layover brings the %{time} back on time", { s = offset(delay), m = m, time = hhmm(after.departure) }), color = AMBER, icon = "schedule" }
    end
    return { text = tr("Late %{s} · still %{left} late for the %{time} after the layover", { s = offset(delay), left = offset(left), time = hhmm(after.departure) }), color = RED, icon = "schedule" }
  end
  return nil
end

-- --- the duty strip ------------------------------------------------------------------------------

-- what comes next on the duty's sheet after `now` (minutes of the day): the next break or
-- relief, and the duty's end; a line of text each, or nil without a sheet
function M.strip(sheet, now)
  if type(sheet) ~= "table" or #sheet == 0 then return nil end
  local out = {}
  local ends
  for _, r in ipairs(sheet) do
    if r.kind == "sign_off" then ends = r end
  end
  for _, r in ipairs(sheet) do
    if r.kind == "break" and (r.to or 0) > now then
      if (r.from or 0) <= now then
        out[#out + 1] = tr("Break until %{time} · %{place}", { time = hm(r.to), place = r.at or "" })
      else
        out[#out + 1] = tr("Break %{from}–%{to} · %{place}", { from = hm(r.from), to = hm(r.to), place = r.at or "" })
      end
      break
    end
    if r.kind == "sign_off" then break end
  end
  if ends then
    local key = ends.relief and "Relief %{time} · %{place}" or "Sign off %{time} · %{place}"
    out[#out + 1] = tr(key, { time = hm(ends.from), place = ends.at or "" })
  end
  if #out == 0 then return nil end
  return out
end

-- --- service control's messages ----------------------------------------------------------------

-- the messages due at `now` (minutes of the day) that `seen` (by id) has not had: service
-- control's decisions for the tour, the reliefs and the breaks of the sheet - each a few
-- minutes before what it is about (or at once when that is past and still under way)
function M.messages(assignment, now, seen)
  local out = {}
  if type(assignment) ~= "table" then return out end
  local function due(id, at, title, text)
    if seen[id] or now < at - AHEAD / 60 then return end
    out[#out + 1] = { id = id, title = title, text = text }
  end
  for i, c in ipairs(type(assignment.controls) == "table" and assignment.controls or {}) do
    local time = hm(c.dep)
    local text
    if c.kind == "short_turn" then
      text = tr("Short-turn the %{time} trip: turn back early (%{m} min saved)", { time = time, m = c.minutes or 0 })
    elseif c.kind == "express" then
      text = tr("Run the %{time} trip out of service to the terminus (%{m} min saved)", { time = time, m = c.minutes or 0 })
    elseif c.kind == "layover" then
      text = tr("The layover before the %{time} trip is cut by %{m} min", { time = time, m = c.minutes or 0 })
    elseif c.kind == "swap" then
      text = (c.bus or "") ~= "" and tr("Bus %{bus} takes over from the %{time} trip", { bus = c.bus, time = time }) or tr("Another bus takes over from the %{time} trip", { time = time })
    end
    -- (a decision for a trip long gone says nothing any more)
    if text and now <= (c.dep or 0) + 30 then due("control" .. i, c.dep or 0, tr("Service control"), text) end
  end
  for i, r in ipairs(type(assignment.sheet) == "table" and assignment.sheet or {}) do
    if r.kind == "break" and now < (r.to or 0) then
      due("break" .. i, r.from or 0, tr("Service control"), tr("Your break at %{place}: %{from}–%{to}", { place = r.at or "", from = hm(r.from), to = hm(r.to) }))
    elseif r.kind == "sign_off" and r.relief and now <= (r.from or 0) then
      due("relief" .. i, r.from or 0, tr("Service control"), tr("You are relieved at %{place} at %{time}", { place = r.at or "", time = hm(r.from) }))
    end
  end
  return out
end

-- --- the fare box --------------------------------------------------------------------------------

-- an amount of the company's money as the company writes it
function M.money(amount, fare)
  local places = math.tointeger(fare.places or 2) or 2
  local n = string.format("%." .. places .. "f", amount)
  if fare.after then return (n:gsub("%.", ",")) .. " " .. (fare.symbol or "") end
  return (fare.symbol or "") .. n
end

-- the fare box's line: the passengers of this trip and of the duty and what they bring, and
-- the load aboard now (`aboard`, where the game says it)
function M.fares(fare, trip_n, duty_n, aboard)
  if type(fare) ~= "table" or not fare.per_boarding then return nil end
  trip_n, duty_n = math.max(trip_n or 0, 0), math.max(duty_n or 0, 0)
  local text = tr("Fare box %{amount} · %{n} boarded this trip · %{total} today", {
    amount = M.money(trip_n * fare.per_boarding, fare), n = math.tointeger(trip_n) or trip_n,
    total = M.money(duty_n * fare.per_boarding, fare),
  })
  if tonumber(aboard) then text = text .. "  ·  " .. tr("%{n} aboard", { n = math.tointeger(aboard) or aboard }) end
  return text
end

-- --- the contract --------------------------------------------------------------------------------

local RAG = { green = GREEN, orange = AMBER, red = RED }
local RAG_WORD = { green = "Green", orange = "Amber", red = "Red" }

-- the contract's line and its colour
function M.contract(c)
  if type(c) ~= "table" or (c.kind or "") == "" then return nil end
  local kind = ({ gross = "Gross cost contract", net = "Net cost contract", hire = "Hire contract" })[c.kind] or "Contract"
  return tr("%{kind} · rated %{rag} · on time −%{early} to +%{late} min", {
    kind = tr(kind), rag = tr(RAG_WORD[c.rag] or "Green"), early = c.early or 1, late = c.late or 5,
  }), RAG[c.rag] or GREEN
end

-- --- on the board ---------------------------------------------------------------------------------

-- the board's elements of the parts switched on (`on`), for the duty `d` now (`clock`: s);
-- `boarded` the passengers of this trip and of the duty. `room`: the whole parts, else only the
-- coach (the most urgent).
function M.elements(on, assignment, d, trips, clock, boarded, room)
  local out = {}
  local a = type(assignment) == "table" and assignment or {}
  local contract = type(a.contract) == "table" and a.contract or nil
  if on.coach then
    local c = M.coach(d, trips, clock, contract)
    if c then
      local children = { { type = "row", gap = 8, children = {
        { type = "icon", name = c.icon, size = 16, color = c.color },
        { type = "text", text = c.text, size = 12.5, weight = "bold", color = "#E8EBF2", grow = true },
      } } }
      if c.note then children[#children + 1] = { type = "text", text = c.note, size = 11.5, color = c.color } end
      out[#out + 1] = { type = "box", color = c.color, children = children }
    end
  end
  if not room then return out end
  -- (duty_strip.lua's strip is the only one on the screen while it is on: docs/omsihub/CREW.md)
  local own = package.loaded["duty_strip"]
  if on.strip and not (own and own.enabled()) then
    local lines = M.strip(a.sheet, math.floor(clock / 60))
    if lines then
      out[#out + 1] = { type = "row", gap = 8, children = {
        { type = "icon", name = "badge", size = 15, color = ORANGE },
        { type = "text", text = table.concat(lines, "  ·  "), size = 11.5, color = SOFT, grow = true },
      } }
    end
  end
  if on.fares then
    local text = M.fares(a.fare, boarded and boarded[1], boarded and boarded[2], boarded and boarded[3])
    if text then
      out[#out + 1] = { type = "row", gap = 8, children = {
        { type = "icon", name = "payments", size = 15, color = ORANGE },
        { type = "text", text = text, size = 11.5, color = SOFT, grow = true },
      } }
    end
  end
  if on.contract then
    local text, color = M.contract(contract)
    if text then
      out[#out + 1] = { type = "row", gap = 8, children = {
        { type = "icon", name = "description", size = 15, color = color },
        { type = "text", text = text, size = 11.5, color = DIM, grow = true },
      } }
    end
  end
  return out
end

return M
