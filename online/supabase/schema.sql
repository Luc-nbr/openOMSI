-- Omsi-Hub online on Supabase: profiles, friends, invites, the opt-in leaderboard and cloud saves.
--
-- The same design as the Neon version it replaces (docs/omsihub/ONLINE.md), on Supabase's own parts:
-- - a player is a Supabase Auth user; `user_id` is that user's id (uuid) and references auth.users,
--   so deleting the user (`delete_my_account`) removes the profile, the friendships and the invites;
-- - row-level security on every table; the signed-in role reads only what concerns the player, and
--   every write goes through a SECURITY DEFINER function that takes the player from the JWT
--   (`hub_me`), never from an argument;
-- - the leaderboard is two functions the anonymous role may call (the website reads them directly);
-- - cloud saves are files in the private bucket `saves`, under users/<user id>/slot1|slot2/, which
--   only their owner can read or write, and only in those two slots.

-- --- tables ----------------------------------------------------------------------------------

create table public.profiles (
  user_id uuid primary key references auth.users (id) on delete cascade,
  display_name text not null check (char_length(display_name) between 3 and 24 and display_name ~ '^[A-Za-z0-9_. -]+$'),
  friend_code text not null unique check (friend_code ~ '^[A-Z0-9]{8}$'),
  status text not null default 'offline' check (status in ('online', 'away', 'busy', 'offline')),
  session_code text check (session_code is null or char_length(session_code) <= 32),
  last_seen timestamptz not null default now(),
  created_at timestamptz not null default now(),
  on_board boolean not null default false,
  points bigint not null default 0,
  level integer not null default 1,
  duties integer not null default 0,
  minutes_driven integer not null default 0,
  km integer not null default 0,
  stats_at timestamptz
);
create unique index profiles_name_ci on public.profiles (lower(display_name));
create index profiles_board_idx on public.profiles (points desc) where on_board;

create table public.friendships (
  requester uuid not null references public.profiles (user_id) on delete cascade,
  addressee uuid not null references public.profiles (user_id) on delete cascade,
  status text not null default 'pending' check (status in ('pending', 'accepted', 'blocked')),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (requester, addressee),
  check (requester <> addressee)
);
create unique index friendships_pair on public.friendships (least(requester, addressee), greatest(requester, addressee));
create index friendships_addressee on public.friendships (addressee);

create table public.invites (
  id bigint generated always as identity primary key,
  from_user uuid not null references public.profiles (user_id) on delete cascade,
  to_user uuid not null references public.profiles (user_id) on delete cascade,
  session_code text not null check (char_length(session_code) between 1 and 32),
  created_at timestamptz not null default now(),
  expires_at timestamptz not null default now() + interval '15 minutes'
);
create index invites_to on public.invites (to_user, expires_at);
create index invites_from on public.invites (from_user, created_at);

alter table public.profiles enable row level security;
alter table public.friendships enable row level security;
alter table public.invites enable row level security;

-- --- helpers ---------------------------------------------------------------------------------

/** The signed-in player, from the JWT; an error when signed out. */
create or replace function public.hub_me() returns uuid
language plpgsql stable security definer set search_path = public, pg_temp as $$
declare v uuid := auth.uid();
begin
  if v is null then raise exception 'not signed in' using errcode = '28000'; end if;
  return v;
end $$;

-- (the helpers the policies and functions share live in `private`, a schema the REST API does not
-- show, so nobody can call them from outside)
create schema if not exists private;
revoke all on schema private from public, anon;
grant usage on schema private to authenticated;

create or replace function private.hub_is_friend(a uuid, b uuid) returns boolean
language sql stable security definer set search_path = public, pg_temp as $$
  select exists (select 1 from public.friendships f where f.status = 'accepted' and ((f.requester = a and f.addressee = b) or (f.requester = b and f.addressee = a)))
$$;

create or replace function private.hub_has_pending(a uuid, b uuid) returns boolean
language sql stable security definer set search_path = public, pg_temp as $$
  select exists (select 1 from public.friendships f where f.status = 'pending' and ((f.requester = a and f.addressee = b) or (f.requester = b and f.addressee = a)))
$$;

create or replace function private.hub_new_code() returns text
language plpgsql security definer set search_path = public, pg_temp as $$
declare a text := 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789'; c text; i int;
begin
  loop
    c := '';
    for i in 1..8 loop c := c || substr(a, 1 + floor(random() * length(a))::int, 1); end loop;
    exit when not exists (select 1 from public.profiles where friend_code = c);
  end loop;
  return c;
end $$;

-- --- what the signed-in player may read ---------------------------------------------------------

create policy profiles_read on public.profiles for select to authenticated
  using (user_id = (select auth.uid()) or private.hub_is_friend((select auth.uid()), user_id) or private.hub_has_pending((select auth.uid()), user_id));
create policy friendships_read on public.friendships for select to authenticated
  using (requester = (select auth.uid()) or (addressee = (select auth.uid()) and status <> 'blocked'));
create policy invites_read on public.invites for select to authenticated
  using ((to_user = (select auth.uid()) and expires_at > now()) or from_user = (select auth.uid()));

-- --- the player's functions ---------------------------------------------------------------------

create or replace function public.create_profile(p_name text) returns public.profiles
language plpgsql security definer set search_path = public, pg_temp as $$
declare me uuid := public.hub_me(); r public.profiles;
begin
  insert into public.profiles (user_id, display_name, friend_code) values (me, btrim(p_name), private.hub_new_code())
  on conflict (user_id) do nothing;
  select * into r from public.profiles where user_id = me;
  return r;
end $$;

create or replace function public.set_display_name(p_name text) returns void
language plpgsql security definer set search_path = public, pg_temp as $$
begin
  update public.profiles set display_name = btrim(p_name) where user_id = public.hub_me();
end $$;

create or replace function public.heartbeat(p_status text, p_session text default null) returns void
language plpgsql security definer set search_path = public, pg_temp as $$
begin
  update public.profiles set status = p_status, session_code = nullif(btrim(coalesce(p_session, '')), ''), last_seen = now() where user_id = public.hub_me();
end $$;

create or replace function public.find_player(p_code text) returns table (user_id uuid, display_name text)
language sql stable security definer set search_path = public, pg_temp as $$
  select p.user_id, p.display_name from public.profiles p where p.friend_code = upper(btrim(p_code)) and public.hub_me() is not null
$$;

create or replace function public.send_friend_request(p_code text) returns text
language plpgsql security definer set search_path = public, pg_temp as $$
declare me uuid := public.hub_me(); other uuid; f public.friendships;
begin
  select user_id into other from public.profiles where friend_code = upper(btrim(p_code));
  if other is null then return 'not_found'; end if;
  if other = me then return 'self'; end if;
  if not exists (select 1 from public.profiles where user_id = me) then return 'no_profile'; end if;
  if (select count(*) from public.friendships where requester = me and created_at > now() - interval '1 hour') >= 20 then return 'too_many'; end if;
  if (select count(*) from public.friendships where (requester = me or addressee = me) and status = 'accepted') >= 500 then return 'full'; end if;
  select * into f from public.friendships where (requester = me and addressee = other) or (requester = other and addressee = me);
  if found then
    if f.status = 'blocked' then return 'sent'; end if;
    if f.status = 'accepted' then return 'already'; end if;
    if f.requester = other then
      update public.friendships set status = 'accepted', updated_at = now() where requester = other and addressee = me;
      return 'accepted';
    end if;
    return 'sent';
  end if;
  insert into public.friendships (requester, addressee) values (me, other);
  return 'sent';
end $$;

create or replace function public.respond_friend_request(p_from uuid, p_accept boolean) returns void
language plpgsql security definer set search_path = public, pg_temp as $$
declare me uuid := public.hub_me();
begin
  if p_accept then
    update public.friendships set status = 'accepted', updated_at = now() where requester = p_from and addressee = me and status = 'pending';
  else
    delete from public.friendships where requester = p_from and addressee = me and status = 'pending';
  end if;
end $$;

create or replace function public.remove_friend(p_other uuid) returns void
language plpgsql security definer set search_path = public, pg_temp as $$
declare me uuid := public.hub_me();
begin
  delete from public.friendships where (requester = me and addressee = p_other) or (requester = p_other and addressee = me and status <> 'blocked');
  delete from public.invites where (from_user = me and to_user = p_other) or (from_user = p_other and to_user = me);
end $$;

create or replace function public.block_player(p_other uuid) returns void
language plpgsql security definer set search_path = public, pg_temp as $$
declare me uuid := public.hub_me();
begin
  if p_other = me or not exists (select 1 from public.profiles where user_id = p_other) then return; end if;
  delete from public.friendships where (requester = me and addressee = p_other) or (requester = p_other and addressee = me);
  delete from public.invites where (from_user = me and to_user = p_other) or (from_user = p_other and to_user = me);
  insert into public.friendships (requester, addressee, status) values (me, p_other, 'blocked');
end $$;

create or replace function public.send_invite(p_to uuid, p_session text) returns text
language plpgsql security definer set search_path = public, pg_temp as $$
declare me uuid := public.hub_me();
begin
  if not private.hub_is_friend(me, p_to) then return 'not_friends'; end if;
  if (select count(*) from public.invites where from_user = me and created_at > now() - interval '1 hour') >= 30 then return 'too_many'; end if;
  delete from public.invites where from_user = me and to_user = p_to;
  insert into public.invites (from_user, to_user, session_code) values (me, p_to, btrim(p_session));
  return 'sent';
end $$;

create or replace function public.dismiss_invite(p_id bigint) returns void
language plpgsql security definer set search_path = public, pg_temp as $$
declare me uuid := public.hub_me();
begin
  delete from public.invites where id = p_id and (to_user = me or from_user = me);
end $$;

create or replace function public.my_friends()
returns table (user_id uuid, display_name text, status text, online boolean, session_code text, last_seen timestamptz, relation text, incoming boolean)
language sql stable security definer set search_path = public, pg_temp as $$
  select p.user_id, p.display_name,
    case when f.status = 'accepted' then p.status else null end,
    f.status = 'accepted' and p.last_seen > now() - interval '2 minutes' and p.status <> 'offline',
    case when f.status = 'accepted' then p.session_code else null end,
    case when f.status = 'accepted' then p.last_seen else null end,
    f.status, f.addressee = public.hub_me()
  from public.friendships f
  join public.profiles p on p.user_id = case when f.requester = public.hub_me() then f.addressee else f.requester end
  where (f.requester = public.hub_me() or (f.addressee = public.hub_me() and f.status <> 'blocked'))
$$;

/** Everything of the player's in these tables (the account itself stays). */
create or replace function public.delete_my_data() returns void
language plpgsql security definer set search_path = public, pg_temp as $$
declare me uuid := public.hub_me();
begin
  delete from public.invites where from_user = me or to_user = me;
  delete from public.friendships where requester = me or addressee = me;
  delete from public.profiles where user_id = me;
end $$;

/** The account itself, and with it (cascade) the profile, friendships and invites. The client
 * deletes the player's cloud saves through the Storage API first. */
create or replace function public.delete_my_account() returns void
language plpgsql security definer set search_path = public, auth, pg_temp as $$
declare me uuid := public.hub_me();
begin
  delete from auth.users where id = me;
end $$;

-- --- the leaderboard ------------------------------------------------------------------------------

create or replace function public.set_leaderboard(p_on boolean) returns boolean
language sql security definer set search_path = public, pg_temp as $$
  with u as (update public.profiles set on_board = coalesce(p_on, false) where user_id = public.hub_me() returning 1)
  select exists (select 1 from u)
$$;

create or replace function public.report_stats(p_points bigint, p_level integer, p_duties integer, p_minutes integer, p_km integer) returns text
language sql security definer set search_path = public, pg_temp as $$
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
$$;

/** The public board: the players who chose to stand on it, the most points first (at most 100). */
create or replace function public.hub_leaderboard(p_limit integer default 50)
returns table (place bigint, display_name text, level integer, points bigint, duties integer, minutes_driven integer, km integer, online boolean)
language sql stable security definer set search_path = public, pg_temp as $$
  select row_number() over (order by p.points desc, p.stats_at asc nulls last, lower(p.display_name)),
         p.display_name, p.level, p.points, p.duties, p.minutes_driven, p.km,
         (p.status <> 'offline' and p.last_seen > now() - interval '2 minutes')
    from public.profiles p
   where p.on_board
   order by 1
   limit least(greatest(coalesce(p_limit, 50), 1), 100)
$$;

create or replace function public.hub_board_counts() returns table (players bigint, online bigint, on_board bigint)
language sql stable security definer set search_path = public, pg_temp as $$
  select count(*),
         count(*) filter (where status <> 'offline' and last_seen > now() - interval '2 minutes'),
         count(*) filter (where on_board)
    from public.profiles
$$;

-- --- the privacy statement -------------------------------------------------------------------------
-- (docs/omsihub/PRIVACY.md, its version in the `privacy-version` marker. The app and the website ask
-- `my_privacy()` at every sign-in and hold the account's features until the player agrees to the
-- current version with `accept_privacy`. A row is never changed or removed by the player; deleting
-- the account removes them with it.)

create table public.privacy_consents (
  user_id uuid not null references auth.users (id) on delete cascade,
  version integer not null check (version between 1 and 1000),
  accepted_at timestamptz not null default now(),
  primary key (user_id, version)
);
alter table public.privacy_consents enable row level security;
create policy privacy_consents_own on public.privacy_consents for select to authenticated
  using (user_id = (select auth.uid()));

/** The player agrees to version `p_version`; the newest version they agreed to. */
create or replace function public.accept_privacy(p_version integer) returns integer
language plpgsql security definer set search_path = public, pg_temp as $$
declare me uuid := (select auth.uid());
begin
  if me is null then raise exception 'not signed in' using errcode = '28000'; end if;
  if p_version is null or p_version < 1 or p_version > 1000 then raise exception 'bad version' using errcode = '22023'; end if;
  insert into public.privacy_consents (user_id, version) values (me, p_version) on conflict do nothing;
  return (select max(version) from public.privacy_consents where user_id = me);
end $$;

/** The newest version the player agreed to, 0 when none. */
create or replace function public.my_privacy() returns integer
language sql stable security definer set search_path = public, pg_temp as $$
  select coalesce(max(version), 0) from public.privacy_consents where user_id = (select auth.uid())
$$;

-- --- who may do what ------------------------------------------------------------------------------

revoke all on public.profiles, public.friendships, public.invites, public.privacy_consents from anon, authenticated;
grant select on public.profiles, public.friendships, public.invites, public.privacy_consents to authenticated;

revoke all on all functions in schema public from public, anon, authenticated;
grant execute on function
  public.hub_me(),
  public.create_profile(text), public.set_display_name(text), public.heartbeat(text, text), public.find_player(text),
  public.send_friend_request(text), public.respond_friend_request(uuid, boolean), public.remove_friend(uuid), public.block_player(uuid),
  public.send_invite(uuid, text), public.dismiss_invite(bigint), public.my_friends(),
  public.delete_my_data(), public.delete_my_account(), public.set_leaderboard(boolean),
  public.report_stats(bigint, integer, integer, integer, integer),
  public.accept_privacy(integer), public.my_privacy()
  to authenticated;
grant execute on function public.hub_leaderboard(integer), public.hub_board_counts() to anon, authenticated;
revoke all on all functions in schema private from public, anon;
grant execute on function private.hub_is_friend(uuid, uuid), private.hub_has_pending(uuid, uuid), private.hub_new_code() to authenticated;

-- --- cloud saves ----------------------------------------------------------------------------------

insert into storage.buckets (id, name, public, file_size_limit)
values ('saves', 'saves', false, 26214400)
on conflict (id) do update set public = false, file_size_limit = 26214400;

-- (only the owner's own folder, users/<user id>/, and in it only the two slots)
create policy saves_read on storage.objects for select to authenticated
  using (bucket_id = 'saves' and (storage.foldername(name))[1] = 'users' and (storage.foldername(name))[2] = (select auth.uid())::text);
create policy saves_insert on storage.objects for insert to authenticated
  with check (bucket_id = 'saves' and (storage.foldername(name))[1] = 'users' and (storage.foldername(name))[2] = (select auth.uid())::text and (storage.foldername(name))[3] in ('slot1', 'slot2'));
create policy saves_update on storage.objects for update to authenticated
  using (bucket_id = 'saves' and (storage.foldername(name))[1] = 'users' and (storage.foldername(name))[2] = (select auth.uid())::text)
  with check (bucket_id = 'saves' and (storage.foldername(name))[1] = 'users' and (storage.foldername(name))[2] = (select auth.uid())::text and (storage.foldername(name))[3] in ('slot1', 'slot2'));
create policy saves_delete on storage.objects for delete to authenticated
  using (bucket_id = 'saves' and (storage.foldername(name))[1] = 'users' and (storage.foldername(name))[2] = (select auth.uid())::text);
