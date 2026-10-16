-- Omsi-Hub's plugin: signing on, as Omsi-Hub's own game had it in the navigator
-- (crates/omsi-app/src/nav_signon.rs) - the driver signs on with a personnel number and a code,
-- signs the duty order, and only then has the duty and its codes in the IBIS. The number and the
-- code are the driver's pass, which Omsi-Hub keeps (`~/.openomsi/personnel.json`) and gives in
-- the assignment; the company's panel (main.lua) shows the stage this module says.
--
-- The stages: "signon" (the number and the code asked), "order" (the duty order to accept),
-- "work" (signed on, the duty accepted). Without a pass in the assignment the driver is at work
-- at once, as with sign-on switched off in Omsi-Hub.

local M = {}

local tr = function(t) return t end
local pass = nil
local typed = { number = "", code = "" }
local wrong = false

M.stage = "work"

function M.setup(translate, assignment)
  tr = translate
  local p = assignment and assignment.personnel
  if type(p) == "table" and (p.number or "") ~= "" and (p.code or "") ~= "" then
    pass = { number = tostring(p.number), code = tostring(p.code) }
    M.stage = "signon"
  end
end

local function hhmm(t)
  local m = math.floor((t or 0) / 60)
  return string.format("%02d:%02d", (m // 60) % 24, m % 60)
end

-- the elements of the stage, for the company's panel; `duty` the assignment
function M.children(duty)
  if M.stage == "signon" then
    return {
      { type = "text", text = tr("Not signed on"), weight = "bold" },
      { type = "input", id = "signon_number", text = typed.number, placeholder = tr("Personnel number"), max = 10 },
      { type = "input", id = "signon_code", text = typed.code, placeholder = tr("Code"), max = 8 },
      { type = "text", text = tr("Not known here. Your number and code are on your pass below."), size = 12, color = "#FF5F57", visible = wrong },
      { type = "button", id = "signon", text = tr("Sign on"), icon = "badge" },
      { type = "text", text = tr("Your pass") .. ": " .. tr("Personnel no. %{number} · code %{code}", { number = pass.number, code = pass.code }), size = 12, color = "#9AA3AD" },
    }
  end
  if M.stage == "order" then
    local rows = {}
    local trips = (omsi.duty and omsi.duty.trips) and omsi.duty.trips() or {}
    for _, t in ipairs(trips) do
      rows[#rows + 1] = { t.line or "", hhmm(t.departure), t.terminus or "" }
    end
    return {
      { type = "text", text = tr("Duty assignment"), weight = "bold" },
      { type = "text", text = tr("Line") .. " " .. (duty.line or "") .. "  ·  " .. tr("Tour") .. " " .. (duty.tour or ""), size = 13 },
      { type = "table", columns = { tr("Line"), tr("Departure"), "" }, widths = { 0.2, 0.25, 0.55 }, rows = rows, size = 12, visible = #rows > 0 },
      { type = "text", text = tr("The duty's codes go to the IBIS as soon as you accept it."), size = 12, color = "#9AA3AD" },
      { type = "button", id = "accept", text = tr("Accept duty"), icon = "task_alt" },
    }
  end
  return {}
end

-- what was typed into a field
function M.change(element, value)
  if element == "signon_number" then typed.number = tostring(value or "") return true end
  if element == "signon_code" then typed.code = tostring(value or "") return true end
  return false
end

-- a click (or Enter in a field) of the panel: `accepted` is called when the duty is accepted.
-- Whether it was this module's.
function M.click(element, accepted)
  if element == "signon" or element == "signon_number" or element == "signon_code" then
    if M.stage ~= "signon" then return true end
    if typed.number == pass.number and typed.code == pass.code then
      M.stage, wrong = "order", false
    elseif element == "signon" or (typed.number ~= "" and typed.code ~= "") then
      wrong = true
    end
    return true
  end
  if element == "accept" and M.stage == "order" then
    M.stage = "work"
    accepted()
    return true
  end
  if element == "signoff" then
    if pass then M.stage, typed = "signon", { number = "", code = "" } end
    return true
  end
  return false
end

-- the driver is at work (signed on, or no sign-on asked)
function M.at_work()
  return M.stage == "work"
end

function M.signed()
  return pass ~= nil
end

return M
