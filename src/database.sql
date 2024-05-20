-- To be used with https://github.com/ruuda/squiller v0.5.0

-- @begin ensure_schema_exists()
create table if not exists teams
( id   integer primary key
, name string  not null
, unique (name)
);

create table if not exists team_memberships
( id           integer primary key
, team_id      integer not null references teams (id),
, member_email string  not null
  -- Every person can be in a given team at most once. They can be in multiple
  -- teams, and the team can have multiple members, this is only about
  -- cardinality.
, unique (team_id, member_email)
);

create table if not exists votes
( id          integer primary key
, voter_email string  not null
, team_id     integer not null references teams (id),
, points      integer not null
  -- Every voter can vote at most once on a team. Without this, you could
  -- sidestep the quadratic voting property.
, unique (voter_email, team_id)
);
-- @end ensure_schema_exists()

-- @query add_team(name: str) ->1 i64
insert into teams (name) values (:name) returning id;

-- @query add_team_member(team_id: i64, member_email: str)
insert into
  team_memberships
  ( team_id
  , member_email
  )
values
  ( :team_id
  , :member_email
  );

-- @query remove_team_member(team_id: i64, member_email: str)
delete from
  team_memberships
where
  team_id = :team_id and member_email = :member_email;

-- @query iter_teams() ->* TeamMember
select
    name as team -- :str
  , member_email -- :str
from
  teams,
  team_memberships
where
  teams.id = team_memberships.team_id
order by
  name asc,
  member_email asc;
