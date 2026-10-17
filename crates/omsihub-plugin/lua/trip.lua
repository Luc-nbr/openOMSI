-- Omsi-Hub's plugin: what a trip of the duty came to, as Omsi-Hub's own game worked it out
-- (crates/omsi-app/src/trip_report.rs): the stops served and how many early, on time and late
-- as OMSI 2 counts them, the stop furthest off the timetable and the average, when the bus came
-- to and left each stop, and the time, distance, jolts, collisions and passengers on the way -
-- from where the bus leaves the trip's first stop (the way there from the depot, or the break
-- at the terminus, is not the trip's). The result is the driver's record of the trip
-- (omsi-launcher-core's `TripRun`), which Omsi-Hub books.

local career = require("career")

local M = {}

-- what is counted all the time; a trip takes its stretch
local total = { seconds = 0, metres = 0, jolts = 0, crashes = 0, boarded = 0 }
local odometer = nil
local cur = nil

local function now()
  return { seconds = total.seconds, metres = total.metres, jolts = total.jolts, crashes = total.crashes, boarded = total.boarded }
end

local function fn(t, name)
  return type(t) == "table" and type(t[name]) == "function"
end

-- every frame of the game with the player in a bus
function M.frame(dt)
  total.seconds = total.seconds + dt
  local km = fn(omsi.bus, "odometer") and omsi.bus.odometer() or nil
  if km and odometer and km >= odometer and km - odometer < 1 then
    total.metres = total.metres + (km - odometer) * 1000
  elseif not km then
    total.metres = total.metres + (omsi.speed and omsi.speed() or 0) / 3.6 * dt
  end
  odometer = km
  if cur and not cur.started then cur.start = now() end
end

function M.jolt() total.jolts = total.jolts + 1 end
function M.crash() total.crashes = total.crashes + 1 end
function M.boarded(n) total.boarded = total.boarded + (tonumber(n) or 0) end

-- the passengers boarded on the trip under way (0 between trips), and on the whole drive
function M.boarded_on_trip()
  if not cur or cur.done then return 0 end
  return math.max(total.boarded - cur.start.boarded, 0)
end

function M.boarded_total()
  return total.boarded
end

-- the line as the trip's displays show it, and its terminus without it ("5 Rathaus": Rathaus)
local function terminus_of(line, terminus)
  terminus = terminus or ""
  if line ~= "" and terminus:sub(1, #line + 1) == line .. " " then terminus = terminus:sub(#line + 2) end
  return terminus
end

-- trip `n` of the duty (from 1) begins: what the timetable says of it
function M.begin(n, line_name, tour)
  cur = nil
  if not (fn(omsi.duty, "trips") and fn(omsi.duty, "trip_stops")) then return end
  local t = omsi.duty.trips()[n]
  if not t then return end
  local line = t.line or ""
  local visits, by_number, last = {}, {}, 0
  for _, s in ipairs(omsi.duty.trip_stops(n) or {}) do
    last = math.max(last, s.number)
    if s.stops ~= false then
      local v = { index = s.number - 1, name = s.name or "", arr = s.arrival or 0, dep = s.departure or 0 }
      visits[#visits + 1] = v
      by_number[s.number] = v
    end
  end
  cur = {
    n = n, line = line, tour = tour or "", line_name = line_name or "", name = t.name or "",
    terminus = terminus_of(line, t.terminus), trips = #omsi.duty.trips(),
    departure = t.departure or 0, arrival = t.arrival or 0, visits = visits, by_number = by_number,
    last = last, arrived = {}, served = {}, reached = 0, furthest = 0, started = false, start = now(),
    done = false,
  }
  -- (the bus stands at a stop of it already - a game started at the trip's first stop: there
  -- since now)
  local d = fn(omsi.duty, "get") and omsi.duty.get() or nil
  if d and d.trip == n and d.at_stop and d.next_stop then M.arrive(d.next_stop.number, d.delay) end
end

function M.current()
  return cur
end

function M.arrive(number, delay)
  if not cur or cur.done or not number then return end
  cur.reached = cur.reached + 1
  cur.furthest = math.max(cur.furthest, number)
  if number > 1 then cur.started = true end
  local v = cur.by_number[number]
  if v and delay then
    cur.arrived[number] = delay
    v.arrived = v.arr + delay
  end
end

function M.depart(number, delay)
  if not cur or cur.done or not number then return end
  cur.started = true
  local v = cur.by_number[number]
  local arrival = cur.arrived[number]
  if v and arrival and delay and number < cur.last then
    cur.served[#cur.served + 1] = { arrival = arrival, departure = delay }
    v.left = v.dep + delay
  end
end

-- the trip ended (`completed`: at its last stop; `delay`: how late the bus came there, where
-- the arrival was not seen): its record, or nil when it has nothing to say (an empty run)
function M.finish(completed, delay, watch)
  if not cur or cur.done then return nil end
  cur.done = true
  local v = cur.by_number[cur.last]
  if completed and v then
    local arrival = cur.arrived[cur.last] or delay
    if arrival then
      cur.served[#cur.served + 1] = { arrival = arrival }
      v.arrived = v.arrived or (v.arr + arrival)
    end
  end
  if cur.line == "" and #cur.visits == 0 then return nil end
  local counts, worst, average = career.tally(cur.served)
  local n = now()
  local visits = {}
  for _, x in ipairs(cur.visits) do
    visits[#visits + 1] = { index = math.tointeger(x.index), name = x.name, arr = x.arr, dep = x.dep, arrived = x.arrived, left = x.left }
  end
  local run = {
    line = cur.line, tour = cur.tour, terminus = cur.terminus,
    trip = math.tointeger(cur.n), trips = math.tointeger(cur.trips),
    departure = cur.departure, arrival = cur.arrival, free = false, completed = completed and true or false,
    planned = #cur.visits, stops = #cur.served, early = counts.early, late = counts.late,
    worst = worst, average = average,
    seconds = math.max(n.seconds - cur.start.seconds, 0), metres = math.max(n.metres - cur.start.metres, 0),
    jolts = math.max(n.jolts - cur.start.jolts, 0), crashes = math.max(n.crashes - cur.start.crashes, 0),
    passengers = math.tointeger(math.max(n.boarded - cur.start.boarded, 0)),
  }
  -- (an empty list is written as `{}` by the JSON writer, which is no list: left out then)
  if #visits > 0 then run.visits = visits end
  if watch then
    local w = watch.between(watch.clock() - run.seconds)
    for k, x in pairs(w) do run[k] = x end
    if #w.offences == 0 then run.offences = nil end
  end
  return run
end

-- the duty went on to another trip without saying this one ended (an openOMSI without
-- `trip_done`): driven to its end when the bus got to its last stops
function M.moved_on(watch)
  if not cur or cur.done then return nil end
  if cur.furthest >= cur.last - 1 and cur.reached > 0 then return M.finish(false, nil, watch) end
  cur.done = true
  return nil
end

function M.forget()
  cur = nil
end

return M
