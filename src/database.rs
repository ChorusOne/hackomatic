// This file was generated by Squiller 0.5.0-dev (unspecified checkout).
// Input files:
// - database.sql

#![allow(unknown_lints)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_question_mark)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::needless_lifetimes)]

use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::hash_map::HashMap;

use sqlite::{
    State::{Done, Row},
    Statement,
};

pub type Result<T> = sqlite::Result<T>;

pub struct Connection<'a> {
    connection: &'a sqlite::Connection,
    statements: HashMap<*const u8, Statement<'a>>,
}

pub struct Transaction<'tx, 'a> {
    connection: &'a sqlite::Connection,
    statements: &'tx mut HashMap<*const u8, Statement<'a>>,
}

pub struct Iter<'i, 'a, T> {
    statement: &'i mut Statement<'a>,
    decode_row: fn(&Statement<'a>) -> Result<T>,
}

impl<'a> Connection<'a> {
    pub fn new(connection: &'a sqlite::Connection) -> Self {
        Self {
            connection,
            // TODO: We could do with_capacity here, because we know the number
            // of queries.
            statements: HashMap::new(),
        }
    }

    /// Begin a new transaction by executing the `BEGIN` statement.
    pub fn begin<'tx>(&'tx mut self) -> Result<Transaction<'tx, 'a>> {
        self.connection.execute("BEGIN;")?;
        let result = Transaction {
            connection: self.connection,
            statements: &mut self.statements,
        };
        Ok(result)
    }
}

impl<'tx, 'a> Transaction<'tx, 'a> {
    /// Execute `COMMIT` statement.
    pub fn commit(self) -> Result<()> {
        self.connection.execute("COMMIT;")
    }

    /// Execute `ROLLBACK` statement.
    pub fn rollback(self) -> Result<()> {
        self.connection.execute("ROLLBACK;")
    }
}

impl<'i, 'a, T> Iterator for Iter<'i, 'a, T> {
    type Item = Result<T>;

    fn next(&mut self) -> Option<Result<T>> {
        match self.statement.next() {
            Ok(Row) => Some((self.decode_row)(self.statement)),
            Ok(Done) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

pub fn ensure_schema_exists(tx: &mut Transaction) -> Result<()> {
    let sql = r#"
        create table if not exists teams
        ( id            integer primary key
        , name          string  not null
        , creator_email string  not null
        , description   string  not null
        , created_at    string  not null
        , unique (name)
        );
        "#;
    let statement = match tx.statements.entry(sql.as_ptr()) {
        Occupied(entry) => entry.into_mut(),
        Vacant(vacancy) => vacancy.insert(tx.connection.prepare(sql)?),
    };
    statement.reset()?;
    match statement.next()? {
        Row => panic!("Query 'ensure_schema_exists' unexpectedly returned a row."),
        Done => {}
    }

    let sql = r#"
        create table if not exists team_memberships
        ( id           integer primary key
        , team_id      integer not null references teams (id)
        , member_email string  not null
          -- Every person can be in a given team at most once. They can be in multiple
          -- teams, and the team can have multiple members, this is only about
          -- cardinality.
        , unique (team_id, member_email)
        );
        "#;
    let statement = match tx.statements.entry(sql.as_ptr()) {
        Occupied(entry) => entry.into_mut(),
        Vacant(vacancy) => vacancy.insert(tx.connection.prepare(sql)?),
    };
    statement.reset()?;
    match statement.next()? {
        Row => panic!("Query 'ensure_schema_exists' unexpectedly returned a row."),
        Done => {}
    }

    let sql = r#"
        create table if not exists votes
        ( id          integer primary key
        , voter_email string  not null
        , team_id     integer not null references teams (id)
        , points      integer not null
          -- Every voter can vote at most once on a team. Without this, you could
          -- sidestep the quadratic voting property.
        , unique (voter_email, team_id)
        );
        "#;
    let statement = match tx.statements.entry(sql.as_ptr()) {
        Occupied(entry) => entry.into_mut(),
        Vacant(vacancy) => vacancy.insert(tx.connection.prepare(sql)?),
    };
    statement.reset()?;
    let result = match statement.next()? {
        Row => panic!("Query 'ensure_schema_exists' unexpectedly returned a row."),
        Done => (),
    };
    Ok(result)
}

pub fn count_teams_by_creator(tx: &mut Transaction, creator_email: &str) -> Result<i64> {
    let sql = r#"
        select count(1) from teams where creator_email = :creator_email;
        "#;
    let statement = match tx.statements.entry(sql.as_ptr()) {
        Occupied(entry) => entry.into_mut(),
        Vacant(vacancy) => vacancy.insert(tx.connection.prepare(sql)?),
    };
    statement.reset()?;
    statement.bind(1, creator_email)?;
    let decode_row = |statement: &Statement| Ok(statement.read(0)?);
    let result = match statement.next()? {
        Row => decode_row(statement)?,
        Done => panic!("Query 'count_teams_by_creator' should return exactly one row."),
    };
    if statement.next()? != Done {
        panic!("Query 'count_teams_by_creator' should return exactly one row.");
    }
    Ok(result)
}

pub fn add_team(
    tx: &mut Transaction,
    name: &str,
    creator_email: &str,
    description: &str,
) -> Result<i64> {
    let sql = r#"
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
          , strftime('%F %TZ', 'now')
          )
        returning
          id;
        "#;
    let statement = match tx.statements.entry(sql.as_ptr()) {
        Occupied(entry) => entry.into_mut(),
        Vacant(vacancy) => vacancy.insert(tx.connection.prepare(sql)?),
    };
    statement.reset()?;
    statement.bind(1, name)?;
    statement.bind(2, creator_email)?;
    statement.bind(3, description)?;
    let decode_row = |statement: &Statement| Ok(statement.read(0)?);
    let result = match statement.next()? {
        Row => decode_row(statement)?,
        Done => panic!("Query 'add_team' should return exactly one row."),
    };
    if statement.next()? != Done {
        panic!("Query 'add_team' should return exactly one row.");
    }
    Ok(result)
}

pub fn add_team_member(tx: &mut Transaction, team_id: i64, member_email: &str) -> Result<()> {
    let sql = r#"
        insert into
          team_memberships
          ( team_id
          , member_email
          )
        values
          ( :team_id
          , :member_email
          );
        "#;
    let statement = match tx.statements.entry(sql.as_ptr()) {
        Occupied(entry) => entry.into_mut(),
        Vacant(vacancy) => vacancy.insert(tx.connection.prepare(sql)?),
    };
    statement.reset()?;
    statement.bind(1, team_id)?;
    statement.bind(2, member_email)?;
    let result = match statement.next()? {
        Row => panic!("Query 'add_team_member' unexpectedly returned a row."),
        Done => (),
    };
    Ok(result)
}

pub fn remove_team_member(tx: &mut Transaction, team_id: i64, member_email: &str) -> Result<()> {
    let sql = r#"
        delete from
          team_memberships
        where
          team_id = :team_id and member_email = :member_email;
        "#;
    let statement = match tx.statements.entry(sql.as_ptr()) {
        Occupied(entry) => entry.into_mut(),
        Vacant(vacancy) => vacancy.insert(tx.connection.prepare(sql)?),
    };
    statement.reset()?;
    statement.bind(1, team_id)?;
    statement.bind(2, member_email)?;
    let result = match statement.next()? {
        Row => panic!("Query 'remove_team_member' unexpectedly returned a row."),
        Done => (),
    };
    Ok(result)
}

#[derive(Debug)]
pub struct Team {
    pub name: String,
    pub creator_email: String,
    pub description: String,
    pub members: String,
}

pub fn iter_teams<'i, 't, 'a>(tx: &'i mut Transaction<'t, 'a>) -> Result<Iter<'i, 'a, Team>> {
    let sql = r#"
        select
            name
          , creator_email
          , description
          , string_agg(member_email order by team_memberships.id, ', ') as members
        from
          teams,
          team_memberships
        where
          teams.id = team_memberships.team_id
        group by
          name,
          creator_email,
          description
        order by
          lower(name) asc;
        "#;
    let statement = match tx.statements.entry(sql.as_ptr()) {
        Occupied(entry) => entry.into_mut(),
        Vacant(vacancy) => vacancy.insert(tx.connection.prepare(sql)?),
    };
    statement.reset()?;
    let decode_row = |statement: &Statement| {
        Ok(Team {
            name: statement.read(0)?,
            creator_email: statement.read(1)?,
            description: statement.read(2)?,
            members: statement.read(3)?,
        })
    };
    let result = Iter {
        statement,
        decode_row,
    };
    Ok(result)
}

// A useless main function, included only to make the example compile with
// Cargo’s default settings for examples.
#[allow(dead_code)]
fn main() {
    let raw_connection = sqlite::open(":memory:").unwrap();
    let mut connection = Connection::new(&raw_connection);

    let tx = connection.begin().unwrap();
    tx.rollback().unwrap();

    let tx = connection.begin().unwrap();
    tx.commit().unwrap();
}
