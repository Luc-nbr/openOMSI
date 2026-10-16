-- Omsi-Hub's plugin: how a trip is judged, as Omsi-Hub's company judges it
-- (omsi-launcher-core's `company::career`: `evaluate`, `stop_credit`, `trip_xp`, the fines) and
-- how OMSI 2 counts a stop early, on time or late. A test runs these against the Rust ones
-- (crates/omsi-launcher-api/src/omsihub/addon.rs), so the card in the game and the company's
-- books say the same.

local M = {}

-- OMSI 2's window: early when the bus left more than two minutes before its time, late when it
-- came more than three after (to the second)
M.LATE_ARRIVAL = 180
M.EARLY_DEPARTURE = -120

-- punctuality fades out beyond the window: to nothing five minutes past it late, three early
local LATE_AFTER, EARLY_BEFORE, LATE_FADE, EARLY_FADE = 180, 120, 300, 180
-- what each comfort event weighs, and the fewest stops a trip is judged as
local HARD_BRAKE, HARD_START, ROUGH_STOP, JOLT, COMFORT_STOPS = 1.0, 0.6, 0.5, 0.5, 10
-- what each safety event takes off its part (of 100)
local RED_LIGHT, SPEEDING, CRASH = 25, 15, 30

-- (as Rust's f64::round: halves away from zero)
local function round(x)
  if x >= 0 then return math.floor(x + 0.5) end
  return -math.floor(-x + 0.5)
end
M.round = round

local function clamp(x, lo, hi)
  if x < lo then return lo end
  if x > hi then return hi end
  return x
end

-- a stop served: `arrival` and `departure` the seconds late (the last stop has no departure)
function M.judged(arrival, departure)
  if departure and round(departure) < M.EARLY_DEPARTURE then return "early" end
  if round(arrival) > M.LATE_ARRIVAL then return "late" end
  return "on_time"
end

-- how far off the timetable a stop was served: as the bus left it, at the last stop as it came
function M.delay(arrival, departure)
  return departure or arrival
end

-- the stops served ({arrival, departure} each): early, on time, late, the stop furthest off the
-- timetable and the average delay
function M.tally(served)
  local counts = { early = 0, on_time = 0, late = 0 }
  local worst, sum, n = nil, 0, 0
  for _, s in ipairs(served) do
    local j = M.judged(s.arrival, s.departure)
    counts[j] = counts[j] + 1
    local d = M.delay(s.arrival, s.departure)
    if worst == nil or math.abs(d) >= math.abs(worst) then worst = d end
    sum, n = sum + d, n + 1
  end
  return counts, worst, n > 0 and sum / n or nil
end

-- how much of a stop counts as punctual (0 - 1)
function M.stop_credit(arrival, departure)
  local late = 1 - clamp((round(arrival) - LATE_AFTER) / LATE_FADE, 0, 1)
  local early = 1
  if departure then early = 1 - clamp((-EARLY_BEFORE - round(departure)) / EARLY_FADE, 0, 1) end
  return math.min(late, early)
end

local function n(x)
  x = tonumber(x) or 0
  return x > 0 and x or 0
end

local function timed(t)
  return not t.free and (t.stops or 0) > 0
end

local function punctuality(t)
  if not timed(t) then return nil end
  local served = {}
  for _, v in ipairs(t.visits or {}) do
    if v.arrived then
      served[#served + 1] = { v.arrived - v.arr, v.left and (v.left - v.dep) or nil }
    end
  end
  if #served > 0 and #served == t.stops then
    local sum = 0
    for _, s in ipairs(served) do sum = sum + M.stop_credit(s[1], s[2]) end
    return 100 * sum / #served
  end
  local on_time = math.max(t.stops - (t.early or 0) - (t.late or 0), 0)
  return 100 * on_time / math.max(t.stops, 1)
end

local function comfort(t)
  local toll = HARD_BRAKE * n(t.hard_brakes) + HARD_START * n(t.hard_starts) + ROUGH_STOP * n(t.rough_stops) + JOLT * n(t.jolts)
  return 100 * math.exp(-toll / math.max(n(t.stops), COMFORT_STOPS))
end

-- the kilometres a trip can have driven in its time (at most 100 km/h, any up to 10 km)
local function believable_km(t)
  local km = (t.metres or 0) / 1000
  if km == km and km > 0 and km <= math.max((t.seconds or 0) / 3600 * 100, 10) then return km end
  return 0
end

function M.trip_xp(t, score)
  local base = 8 * believable_km(t) + 4 * n(t.stops) + clamp(t.passengers or 0, 0, 200) + (t.completed and 20 or 0)
  local weighed = base * (0.25 + 0.75 * math.min(score, 100) / 100)
  return round(weighed) + (score >= 90 and 40 or 0)
end

function M.grade(score)
  if score >= 90 then return "Excellent", 5 end
  if score >= 75 then return "Good", 4 end
  if score >= 60 then return "Fair", 3 end
  if score >= 40 then return "Poor", 2 end
  return "Bad", 1
end

-- a trip judged (`t` as the driver's record keeps it): its parts, score, grade and experience
function M.evaluate(t)
  local c = comfort(t)
  local s = clamp(100 - (RED_LIGHT * n(t.red_lights) + SPEEDING * n(t.speeding) + CRASH * n(t.crashes)), 0, 100)
  local p = punctuality(t)
  local score
  if p then score = 0.4 * p + 0.3 * c + 0.3 * s else score = 0.5 * c + 0.5 * s end
  if not t.completed then score = score - 10 end
  score = math.tointeger(round(clamp(score, 0, 100)))
  local grade, stars = M.grade(score)
  return {
    punctuality = p and math.tointeger(round(p)) or nil, comfort = math.tointeger(round(c)),
    safety = math.tointeger(round(s)), score = score, grade = grade, stars = stars,
    xp = math.tointeger(M.trip_xp(t, score)),
  }
end

-- --- the fines ---------------------------------------------------------------------------------

-- what a speed camera allows over the limit: 3 km/h up to 100 km/h, 3 % above
function M.tolerance(speed)
  if speed <= 100 then return 3 end
  return math.ceil(speed * 0.03)
end

-- how far over the limit a camera counts `speed` (whole km/h; nil within the tolerance)
function M.over_limit(speed, limit)
  if not (speed == speed and speed ~= math.huge and limit and limit > 0) then return nil end
  local counted = math.floor(speed - M.tolerance(speed))
  if counted > limit then return counted - limit end
  return nil
end

local TOWN = { { 10, 120 }, { 15, 190 }, { 20, 270 }, { 25, 360 }, { 30, 450 }, { 40, 700 }, { 50, 1000 }, { 60, 1400 }, { 70, 1800 } }
local OUT = { { 10, 100 }, { 15, 160 }, { 20, 230 }, { 25, 310 }, { 30, 400 }, { 40, 600 }, { 50, 900 }, { 60, 1200 }, { 70, 1600 } }

-- the fine (cents) for `over` km/h too fast, in town (a limit up to 50) or outside it
function M.speeding_fine(over, town)
  if over <= 0 then return 0 end
  for _, row in ipairs(town and TOWN or OUT) do
    if over <= row[1] then return row[2] * 100 end
  end
  return (town and 2500 or 2200) * 100
end

-- a red light run: 90 euros, 200 when it had been red for more than a second
function M.red_light_fine(red_for)
  if red_for > 1 then return 20000 end
  return 9000
end

return M
