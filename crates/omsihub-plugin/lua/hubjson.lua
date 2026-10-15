-- Omsi-Hub's plugin: values as JSON text, written here rather than with omsi.json.encode so
-- that the handshake and the reports also reach Omsi-Hub from an openOMSI without it.

local M = {}

local escapes = { ['"'] = '\\"', ["\\"] = "\\\\", ["\b"] = "\\b", ["\f"] = "\\f", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }

local function str(s)
  return '"' .. s:gsub('[%c"\\]', function(c)
    return escapes[c] or string.format("\\u%04x", c:byte())
  end) .. '"'
end

-- a table is a list when its keys are exactly 1..n
local function is_list(t)
  local n = 0
  for k in pairs(t) do
    if type(k) ~= "number" or k < 1 or k % 1 ~= 0 then return false end
    n = n + 1
  end
  return n > 0 and n == #t
end

local function encode(v, depth)
  local t = type(v)
  if t == "nil" then return "null" end
  if t == "boolean" then return tostring(v) end
  if t == "number" then
    if v ~= v or v == math.huge or v == -math.huge then return "null" end
    if math.type(v) == "integer" then return string.format("%d", v) end
    return string.format("%.17g", v)
  end
  if t == "string" then return str(v) end
  if t ~= "table" or depth > 16 then return "null" end
  local out = {}
  if is_list(v) then
    for i = 1, #v do out[i] = encode(v[i], depth + 1) end
    return "[" .. table.concat(out, ",") .. "]"
  end
  local keys = {}
  for k in pairs(v) do
    if type(k) == "string" or type(k) == "number" then keys[#keys + 1] = tostring(k) end
  end
  table.sort(keys)
  for _, k in ipairs(keys) do
    local x = v[k]
    if x == nil then x = v[tonumber(k)] end
    out[#out + 1] = str(k) .. ":" .. encode(x, depth + 1)
  end
  return "{" .. table.concat(out, ",") .. "}"
end

function M.encode(v)
  return encode(v, 0)
end

return M
