-- Omsi-Hub's plugin: the duty strip of a company's duty (Luc, 2026-10-16) - the piece of work
-- under way, the next break and the next relief with where, when and how long until then, the
-- end of the duty, what changed today (an extra duty, overtime, a changed relief, a swap), the
-- bus's fuel or charge with "return to the depot" when it runs low; and the driver's side of the
-- bus: the handover notes left on it (shown at sign-on), a note of one's own for the next driver,
-- and the walk-around at sign-off whose defects go to the company's workshop. A collision the
-- game sees is logged to the workshop by itself.
--
-- Self-contained: a panel of its own (`omsihub_strip`), its data from the assignment's `crew`
-- (Omsi-Hub's `bridge::Crew`, made from the RosterFlow duty the company launched), and what it
-- reports goes through the `report` main.lua hands it. Switched off by the assignment
-- (`crew.strip = false`) or by another module that draws a strip of its own (`M.set_enabled`):
-- there is only ever one duty strip on the screen.

local M = {}

local PANEL = "omsihub_strip"
local GREEN, AMBER, RED, DIM, ACCENT = "#37D67A", "#F0B429", "#FF5F57", "#9AA3AD", "#F47F30"
-- (a stand shorter than this is a layover, not a break; the relief is told this long before)
local BREAK_MIN = 10
local RELIEF_WARN = 10
-- (collisions are logged at most this often, s)
local FAULT_EVERY = 120

local tr = function(t) return t end
local crew = nil
local report = function() end
local notify = function() end

local enabled = true
local shown = false
-- what the driver has open: nil, "note" or "check"
local open = nil
local note = { template = nil, text = "" }
local check = { ticked = {}, text = "", done = false }
local told = {}
local fuel = { level = nil, low = false, most = nil, told = false }
local fault = { base = nil, at = -FAULT_EVERY, clock = 0 }
local look = 0

-- `translate` the plugin's texts, `c` the assignment's crew (nil: not a company's duty),
-- `send(kind, fields)` a report to Omsi-Hub, `say(icon, title, text)` a notification
function M.setup(translate, c, send, say)
  tr, crew, report, notify = translate, c, send or report, say or notify
  if type(crew) ~= "table" or type(crew.rows) ~= "table" then crew = nil end
  enabled = crew ~= nil and crew.strip ~= false
end

-- whether the strip is shown (another module may take the place: `set_enabled(false)`)
function M.enabled()
  return enabled
end

function M.set_enabled(on)
  enabled = on and crew ~= nil
  if not enabled and shown and omsi.ui and omsi.ui.remove then
    omsi.ui.remove(PANEL)
    shown = false
  end
end

local function fn(t, name)
  return type(t) == "table" and type(t[name]) == "function"
end

local function hhmm(m)
  m = math.floor(m or 0)
  return string.format("%02d:%02d", (m // 60) % 24, m % 60)
end

-- the game's clock in minutes of the day
local function minute()
  local i = fn(omsi, "info") and omsi.info() or nil
  local s = type(i) == "table" and tonumber(i.clock) or 0
  return (s or 0) / 60
end

local function until_text(m, now)
  local left = math.max(math.ceil(m - now), 0)
  if left >= 60 then return tr("in %{h} h %{m} min", { h = left // 60, m = left % 60 }) end
  return tr("in %{m} min", { m = left })
end

-- --- what the duty looks like now ---------------------------------------------------------------

-- the piece under way at minute `now`: its text and colour
local function piece(now)
  local rows = crew.rows
  local first, last = rows[1], rows[#rows]
  if first and now < first.from then
    return tr("Sign on at %{place} at %{time}", { place = first.at, time = hhmm(first.from) }), DIM
  end
  for _, r in ipairs(rows) do
    if now >= r.from and now < math.max(r.to, r.from + 1) then
      if r.kind == "trip" and r.empty then return tr("Empty run to %{place} until %{time}", { place = r.to_place, time = hhmm(r.to) }), DIM end
      if r.kind == "trip" then return tr("Line %{line} to %{place} until %{time}", { line = r.line, place = r.to_place, time = hhmm(r.to) }), GREEN end
      if r.kind == "break" then return tr("Break at %{place} until %{time}", { place = r.at, time = hhmm(r.to) }), AMBER end
    end
  end
  if last and now >= last.from then return tr("Duty finished"), GREEN end
  return tr("On duty"), GREEN
end

-- the next break (or the one under way) after minute `now`
local function next_break(now)
  for _, r in ipairs(crew.rows) do
    if r.kind == "break" and r.to - r.from >= BREAK_MIN and r.to > now then return r end
  end
  return nil
end

-- the next relief: where the driver takes a bus over or hands it on
local function next_relief(now)
  for _, r in ipairs(crew.rows) do
    if (r.kind == "sign_on" or r.kind == "sign_off") and r.relief and r.from >= now then return r end
  end
  return nil
end

-- --- the bus: fuel and faults --------------------------------------------------------------------

-- the fuel (or the charge) left, 0 to 1, where the bus tells it
local function fuel_level()
  if not omsi.bus then return nil end
  -- an electric bus's charge, where its script has one (0..1 or 0..100)
  if fn(omsi.bus, "get_vars") then
    local v = omsi.bus.get_vars({ "elec_battery_soc", "battery_soc", "soc", "engine_tank_capacity" })
    if type(v) == "table" then
      for _, k in ipairs({ "elec_battery_soc", "battery_soc", "soc" }) do
        local x = tonumber(v[k])
        if x then return x > 1 and math.min(x / 100, 1) or math.max(x, 0), "charge" end
      end
      local cap = tonumber(v.engine_tank_capacity)
      if cap and cap > 0 then fuel.most = cap end
    end
  end
  local litres = fn(omsi.bus, "fuel") and tonumber(omsi.bus.fuel()) or nil
  if not litres then return nil end
  -- (without the tank's size: the most seen since the drive began, the tank as it came)
  fuel.most = math.max(fuel.most or 0, litres)
  if fuel.most <= 0 then return nil end
  return math.min(litres / fuel.most, 1), "fuel"
end

-- what the bus says of itself, looked at once a second: the fuel, a collision
function M.frame(dt)
  if not crew then return end
  look = look - (dt or 0)
  fault.clock = fault.clock + (dt or 0)
  if look > 0 then return end
  look = 1
  local level, kind = fuel_level()
  fuel.level, fuel.kind = level, kind
  local low = level ~= nil and level < (tonumber(crew.low_fuel) or 0.15)
  if low and not fuel.told then
    fuel.told = true
    notify("local_gas_station", tr("Fuel low"), tr("Return to the depot to refuel"))
  elseif not low and level and level > (tonumber(crew.low_fuel) or 0.15) + 0.1 then
    fuel.told = false
  end
  fuel.low = low
  -- a collision: logged to the workshop by itself (once in a while, not every bump)
  if fn(omsi.bus, "damage") then
    local d = omsi.bus.damage()
    local n = type(d) == "table" and tonumber(d.crashes) or nil
    if n then
      if fault.base == nil then fault.base = n end
      if n > fault.base and fault.clock - fault.at >= FAULT_EVERY then
        fault.base, fault.at = n, fault.clock
        report("fault", { defect = "other", text = "", minutes = tonumber(d.repair_minutes), bus_id = crew.bus })
      elseif n > fault.base then
        fault.base = n
      end
    end
  end
end

-- --- the strip ----------------------------------------------------------------------------------

local function line(icon, label, value, color)
  return { type = "row", children = {
    { type = "icon", name = icon, color = color or DIM, size = 16 },
    { type = "text", text = label, size = 12, color = DIM, wrap = false },
    { type = "text", text = value, size = 13, color = color, grow = true, align = "right" },
  } }
end

local function note_form(children)
  local function add(x) children[#children + 1] = x end
  add({ type = "divider" })
  add({ type = "text", text = tr("A note for the next driver of bus %{bus}", { bus = crew.bus_number or "" }), weight = "bold", size = 13 })
  for _, t in ipairs(crew.templates or {}) do
    add({ type = "checkbox", id = "strip_tpl_" .. t[1], text = t[2], checked = note.template == t[1] })
  end
  add({ type = "input", id = "strip_note_text", text = note.text, placeholder = tr("Or type a note"), max = tonumber(crew.text_max) or 140 })
  add({ type = "row", children = {
    { type = "button", id = "strip_note_send", text = tr("Leave the note"), icon = "sticky_note_2", grow = true },
    { type = "button", id = "strip_close", text = tr("Close"), grow = true },
  } })
end

local function check_form(children)
  local function add(x) children[#children + 1] = x end
  add({ type = "divider" })
  add({ type = "text", text = tr("Walk-around check of bus %{bus}", { bus = crew.bus_number or "" }), weight = "bold", size = 13 })
  add({ type = "text", text = tr("Tick what is not in order: the workshop gets it."), size = 12, color = DIM })
  for _, k in ipairs(crew.checks or {}) do
    add({ type = "checkbox", id = "strip_chk_" .. k[1], text = k[2], checked = check.ticked[k[1]] == true })
  end
  add({ type = "input", id = "strip_chk_text", text = check.text, placeholder = tr("What is wrong (optional)"), max = tonumber(crew.text_max) or 140 })
  add({ type = "row", children = {
    { type = "button", id = "strip_chk_send", text = tr("Report defects"), icon = "build", grow = true },
    { type = "button", id = "strip_chk_ok", text = tr("All in order"), icon = "task_alt", grow = true },
  } })
end

-- the strip's elements at minute `now`
function M.rows(now)
  local children = {}
  local function add(x) children[#children + 1] = x end
  local text, color = piece(now)
  add({ type = "row", children = {
    { type = "icon", name = "badge", color = ACCENT },
    { type = "text", text = text, weight = "bold", color = color, grow = true },
  } })
  local b = next_break(now)
  if b then
    local when = b.from <= now and tr("now, until %{time}", { time = hhmm(b.to) }) or (hhmm(b.from) .. " · " .. until_text(b.from, now))
    add(line("coffee", tr("Next break"), b.at .. "  " .. when))
  end
  local r = next_relief(now)
  if r then
    local soon = r.from - now <= RELIEF_WARN
    add(line("swap_horiz", tr("Next relief"), r.at .. "  " .. hhmm(r.from) .. " · " .. until_text(r.from, now), soon and AMBER or nil))
    local key = r.kind .. hhmm(r.from)
    if soon and not told[key] then
      told[key] = true
      notify("swap_horiz", tr("Relief due"), tr("At %{place} at %{time}", { place = r.at, time = hhmm(r.from) }))
    end
  end
  local last = crew.rows[#crew.rows]
  if last then add(line("flag", tr("End of duty"), hhmm(last.from) .. "  " .. (last.at or ""))) end
  for _, ch in ipairs(crew.changes or {}) do
    add({ type = "text", text = "• " .. ch, size = 12, color = AMBER })
  end
  if fuel.level then
    local label = fuel.kind == "charge" and tr("Charge") or tr("Fuel")
    add({ type = "row", children = {
      { type = "icon", name = fuel.kind == "charge" and "battery_charging_full" or "local_gas_station", color = fuel.low and RED or DIM, size = 16 },
      { type = "text", text = label .. " " .. math.floor(fuel.level * 100 + 0.5) .. " %", size = 12, color = fuel.low and RED or DIM, wrap = false },
      { type = "bar", value = fuel.level, grow = true, color = fuel.low and RED or GREEN },
    } })
    if fuel.low then add({ type = "text", text = tr("Return to the depot to refuel"), size = 12, color = RED }) end
  end
  if open == "note" then
    note_form(children)
  elseif open == "check" then
    check_form(children)
  else
    add({ type = "row", children = {
      { type = "button", id = "strip_note", text = tr("Leave a note"), icon = "sticky_note_2", grow = true },
      { type = "button", id = "strip_check", text = tr("Defect check"), icon = "build", grow = true },
    } })
  end
  return children
end

-- the strip now (`on`: the driver is at work on the company's duty)
function M.update(on)
  if not (crew and omsi.ui and omsi.ui.set) then return end
  if not (enabled and on) then
    if shown then
      omsi.ui.remove(PANEL)
      shown = false
    end
    return
  end
  omsi.ui.set(PANEL, { anchor = "bottom_left", x = 16, y = 16, width = 340, accent = ACCENT, draggable = true, children = M.rows(minute()) })
  shown = true
end

-- --- sign-on and sign-off -----------------------------------------------------------------------

-- the driver signed on (or is in the bus at the duty's start): the notes left on the bus, once
function M.signed_on()
  if not crew then return end
  omsi.data.strip_seen = omsi.data.strip_seen or {}
  for _, n in ipairs(crew.notes or {}) do
    if not omsi.data.strip_seen[n] then
      omsi.data.strip_seen[n] = true
      notify("sticky_note_2", tr("A note was left for you"), n)
    end
  end
end

-- the driver signs off (or the duty ended): the walk-around opens, once
function M.sign_off()
  if not crew or check.done then return end
  open = "check"
  M.update(true)
end

local function send_check(all_fine)
  local sent = 0
  if not all_fine then
    for _, k in ipairs(crew.checks or {}) do
      if check.ticked[k[1]] then
        report("defect", { defect = k[1], text = check.text, bus_id = crew.bus })
        sent = sent + 1
      end
    end
    if sent == 0 and check.text ~= "" then
      report("defect", { defect = "other", text = check.text, bus_id = crew.bus })
      sent = 1
    end
  end
  check = { ticked = {}, text = "", done = true }
  open = nil
  if sent > 0 then
    notify("build", tr("Defects reported"), tr("%{n} reported to the workshop", { n = sent }))
  end
end

-- a click on the strip: whether it was the strip's
function M.click(p, element)
  if p ~= PANEL or not crew then return false end
  element = element or ""
  if element == "strip_note" then open = "note"
  elseif element == "strip_check" then open = "check"
  elseif element == "strip_close" then open = nil
  elseif element == "strip_note_send" or element == "strip_note_text" then
    if note.template or note.text ~= "" then
      report("note", { template = note.template or "", text = note.text, bus_id = crew.bus })
      notify("sticky_note_2", tr("Note left"), tr("The next driver of bus %{bus} sees it", { bus = crew.bus_number or "" }))
      note = { template = nil, text = "" }
      open = nil
    end
  elseif element == "strip_chk_send" then send_check(false)
  elseif element == "strip_chk_ok" then send_check(true)
  end
  M.update(true)
  return true
end

-- a checkbox ticked or a field typed into: whether it was the strip's
function M.change(p, element, value)
  if p ~= PANEL or not crew then return false end
  element = element or ""
  local tpl = element:match("^strip_tpl_(.+)$")
  local chk = element:match("^strip_chk_(.+)$")
  if tpl then
    note.template = value and tpl or nil
  elseif element == "strip_note_text" then
    note.text = tostring(value or "")
  elseif element == "strip_chk_text" then
    check.text = tostring(value or "")
  elseif chk then
    check.ticked[chk] = value and true or nil
  else
    return false
  end
  return true
end

function M.stop()
  if shown and omsi.ui and omsi.ui.remove then omsi.ui.remove(PANEL) end
  shown = false
end

return M
