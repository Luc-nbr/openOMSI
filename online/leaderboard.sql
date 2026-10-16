-- Omsi-Hub online: the public leaderboard (opt-in).
--
-- A signed-in player can choose to stand on the leaderboard. Only then do their display name and
-- their service record (points, level, duties, time driven, km) become public, on the website and
-- in the app. Off by default; switching it off takes the player off the board at once. No email
-- and no friend code is ever public.
--
-- The app reports the service record with `report_stats` (signed in, through the Data API, as every
-- other write). The website reads the board through the Neon Function `board`
-- (online/functions/board), which calls `hub_leaderboard` and `hub_board_counts` with the owner's
-- connection: neither is granted to `anonymous` or `authenticated`.

alter table public.profiles
  add column if not exists on_board boolean not null default false,
  add column if not exists points bigint not null default 0,
  add column if not exists level integer not null default 1,
  add column if not exists duties integer not null default 0,
  add column if not exists minutes_driven integer not null default 0,
  add column if not exists km integer not null default 0,
  add column if not exists stats_at timestamptz;

create index if not exists profiles_board_idx on public.profiles (points desc) where on_board;

/** Stand on the leaderboard, or leave it. Returns whether the player has a profile. */
create or replace function public.set_leaderboard(p_on boolean)
 returns boolean
 language sql
 security definer
 set search_path to 'public', 'pg_temp'
as $function$
  with u as (update public.profiles set on_board = coalesce(p_on, false) where user_id = public.hub_me() returning 1)
  select exists (select 1 from u)
$function$;

/** The player's service record, as the app counts it. Returns `ok`, `too_soon` (more than once in
 * 30 s), `bad` (out of range) or `no_profile`. One statement: the update runs only when the
 * answer is `ok`. */
create or replace function public.report_stats(p_points bigint, p_level integer, p_duties integer, p_minutes integer, p_km integer)
 returns text
 language sql
 security definer
 set search_path to 'public', 'pg_temp'
as $function$
  with me as (select public.hub_me() as id),
  cur as (select p.stats_at from public.profiles p join me on p.user_id = me.id),
  verdict as (
    select case
      when p_points is null or p_points < 0 or p_points > 1000000000
        or p_level is null or p_level < 1 or p_level > 1000
        or p_duties is null or p_duties < 0 or p_duties > 1000000
        or p_minutes is null or p_minutes < 0 or p_minutes > 100000000
        or p_km is null or p_km < 0 or p_km > 100000000 then 'bad'
      when not exists (select 1 from cur) then 'no_profile'
      when (select stats_at from cur) > now() - interval '30 seconds' then 'too_soon'
      else 'ok' end as r),
  u as (
    update public.profiles p
       set points = p_points, level = p_level, duties = p_duties, minutes_driven = p_minutes, km = p_km, stats_at = now()
      from me, verdict
     where p.user_id = me.id and verdict.r = 'ok'
    returning 1)
  select r from verdict
$function$;

/** The board: the players who chose to stand on it, the most points first (at most 100). */
create or replace function public.hub_leaderboard(p_limit integer default 50)
 returns table (place bigint, display_name text, level integer, points bigint, duties integer, minutes_driven integer, km integer, online boolean)
 language sql
 stable
 security definer
 set search_path to 'public', 'pg_temp'
as $function$
  select row_number() over (order by p.points desc, p.stats_at asc nulls last, lower(p.display_name)),
         p.display_name, p.level, p.points, p.duties, p.minutes_driven, p.km,
         (p.status <> 'offline' and p.last_seen > now() - interval '2 minutes')
    from public.profiles p
   where p.on_board
   order by 1
   limit least(greatest(coalesce(p_limit, 50), 1), 100)
$function$;

/** The counts beside the board: accounts, online now, standing on the board. */
create or replace function public.hub_board_counts()
 returns table (players bigint, online bigint, on_board bigint)
 language sql
 stable
 security definer
 set search_path to 'public', 'pg_temp'
as $function$
  select count(*),
         count(*) filter (where status <> 'offline' and last_seen > now() - interval '2 minutes'),
         count(*) filter (where on_board)
    from public.profiles
$function$;

revoke all on function public.set_leaderboard(boolean), public.report_stats(bigint, integer, integer, integer, integer),
  public.hub_leaderboard(integer), public.hub_board_counts() from public;
grant execute on function public.set_leaderboard(boolean), public.report_stats(bigint, integer, integer, integer, integer) to authenticated;
