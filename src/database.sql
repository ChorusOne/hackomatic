-- To be used with https://github.com/ruuda/squiller v0.5.0

-- @begin ensure_schema_exists()
create table if not exists teams
( id            integer primary key
, name          string  not null
, creator_email string  not null
, description   string  not null
, created_at    string  not null
, unique (name)
);

create table if not exists team_memberships
( id           integer primary key
, team_id      integer not null references teams (id)
, member_email string  not null
  -- Every person can be in a given team at most once. They can be in multiple
  -- teams, and the team can have multiple members, this is only about
  -- cardinality.
, unique (team_id, member_email)
);

create table if not exists votes
( id          integer primary key
, voter_email string  not null
, team_id     integer not null references teams (id)
, points      integer not null
  -- Every voter can vote at most once on a team. Without this, you could
  -- sidestep the quadratic voting property.
, unique (voter_email, team_id)
);

create table if not exists progress
( id         integer primary key
, created_at string not null
, phase      string not null
);
-- @end ensure_schema_exists()

-- @query get_current_phase() ->? str
select phase from progress order by id desc limit 1;

-- @query set_current_phase(phase: str)
insert into
  progress (phase, created_at)
values
  (:phase, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

-- @query count_teams_by_creator(creator_email: str) ->1 i64
select count(1) from teams where creator_email = :creator_email;

-- @query add_team(
--    name: str,
--    creator_email: str,
--    description: str,
-- ) ->1 i64
insert into
  teams
  ( name
  , creator_email
  , description
  , created_at
  )
values
  ( :name
  , :creator_email
  , :description
  , strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  )
returning
  id;

-- @query delete_team(team_id: i64)
delete from teams where id = :team_id;

-- @query add_team_member(team_id: i64, member_email: str)
insert into
  team_memberships
  ( team_id
  , member_email
  )
values
  ( :team_id
  , :member_email
  )
on conflict
  -- If the user is already a member, making them a member again does nothing.
  do nothing
;

-- @query remove_team_member(team_id: i64, member_email: str)
delete from
  team_memberships
where
  team_id = :team_id and member_email = :member_email;

-- @query iter_teams() ->* Team
select
    id            -- :i64
  , name          -- :str
  , creator_email -- :str
  , description   -- :str
  -- Previously we selected the members as well here with string_agg, but that
  -- is not supported by the version of SQLite that Ubuntu ships :'(.
from
  teams
order by
  lower(name) asc;

-- @query iter_team_members(team_id: i64) ->* str
select
  member_email
from
  team_memberships
where
  team_id = :team_id
order by
  id asc;
