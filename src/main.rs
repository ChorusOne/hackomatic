// Hack-o-matic -- A webapp for facilitating remote and on-site hackathons
// Copyright 2024 Chorus One

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

use std::io::Cursor;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use tiny_http::{HeaderField, Method, Request, Server};

use config::Config;
use database as db;
use endpoints::{internal_error, not_found, service_unavailable};

mod config;
mod database;
mod endpoints;

type Response = tiny_http::Response<Cursor<Vec<u8>>>;

#[derive(Copy, Clone, PartialEq, Eq)]
enum Phase {
    Registration,
    Presentation,
    Evaluation,
    Revelation,
    Celebration,
}

impl Phase {
    fn from_str(name: &str) -> Option<Phase> {
        let result = match name {
            "registration" => Phase::Registration,
            "presentation" => Phase::Presentation,
            "evaluation" => Phase::Evaluation,
            "revelation" => Phase::Revelation,
            "celebration" => Phase::Celebration,
            _ => return None,
        };
        Some(result)
    }

    fn to_str(&self) -> &'static str {
        match self {
            Phase::Registration => "registration",
            Phase::Presentation => "presentation",
            Phase::Evaluation => "evaluation",
            Phase::Revelation => "revelation",
            Phase::Celebration => "celebration",
        }
    }

    fn prev(&self) -> Phase {
        match self {
            Phase::Registration => Phase::Registration,
            Phase::Presentation => Phase::Registration,
            Phase::Evaluation => Phase::Presentation,
            Phase::Revelation => Phase::Evaluation,
            Phase::Celebration => Phase::Revelation,
        }
    }

    fn next(&self) -> Phase {
        match self {
            Phase::Registration => Phase::Presentation,
            Phase::Presentation => Phase::Evaluation,
            Phase::Evaluation => Phase::Revelation,
            Phase::Revelation => Phase::Celebration,
            Phase::Celebration => Phase::Celebration,
        }
    }
}

fn load_phase(tx: &mut db::Transaction) -> db::Result<Phase> {
    let result = db::get_current_phase(tx)?
        .and_then(|p| Phase::from_str(&p))
        .unwrap_or(Phase::Registration);
    Ok(result)
}

fn load_config() -> Config {
    let mut args = std::env::args();

    // Skip the program name.
    args.next();

    let config_fname = match args.next() {
        Some(fname) => fname,
        None => panic!("Expected config file path as first argument."),
    };

    let config_toml = match std::fs::read_to_string(&config_fname) {
        Ok(string) => string,
        Err(err) => panic!("Failed to read {config_fname:?}: {err:?}"),
    };

    match toml::from_str(&config_toml) {
        Ok(config) => config,
        Err(err) => panic!("Failed to parse {config_fname:?}: {err:?}"),
    }
}

fn init_database(raw_connection: &sqlite::Connection) -> db::Result<db::Connection> {
    // Change the database to WAL mode if it wasn't already. Set the busy
    // timeout to 30 milliseconds, so readers and writers can wait for each
    // other a little bit. We also have a retry loop around the request handler.
    raw_connection.execute("PRAGMA locking_mode = NORMAL;")?;
    raw_connection.execute("PRAGMA busy_timeout = 30;")?;
    raw_connection.execute("PRAGMA journal_mode = WAL;")?;
    raw_connection.execute("PRAGMA foreign_keys = TRUE;")?;
    let mut connection = db::Connection::new(raw_connection);
    let mut tx = connection.begin()?;
    db::ensure_schema_exists(&mut tx)?;
    tx.commit()?;
    Ok(connection)
}

pub struct User {
    email: String,
    is_admin: bool,
}

impl User {
    /// Whether to display the outcome of the vote to the user.
    ///
    /// In the revelation phase, only the admin gets to see the
    /// totals so that people can't run ahead and check who won
    /// during the ceremony. But afterwards, everybody can check
    /// at their own pace.
    fn can_see_outcome(&self, phase: Phase) -> bool {
        match phase {
            Phase::Revelation => self.is_admin,
            Phase::Celebration => true,
            _ => false,
        }
    }
}

fn handle_request(
    config: &Config,
    connection: &mut db::Connection,
    request: &mut Request,
    log_line: &mut String,
) -> db::Result<Response> {
    // Figure out who the user is. In debug mode we fall back to a default.
    let header_x_email = HeaderField::from_str("X-Email").unwrap();
    let mut email = None;
    for header in request.headers() {
        if header.field == header_x_email {
            // We need to clone the value, because later on we might need to
            // read the request body, and we can't do that with a reference to
            // a header.
            email = Some(header.value.to_string());
        }
    }
    let email = match email {
        Some(email) => email,
        None => match config.debug.unsafe_default_email.clone() {
            Some(fallback) => fallback,
            None => {
                return Ok(
                    Response::from_string("Missing authentication header.").with_status_code(401)
                )
            }
        },
    };

    *log_line = format!("{:4?} {} {}", request.method(), request.url(), email);

    let user = User {
        is_admin: email == config.app.admin_email,
        email,
    };

    let url_inner = match request.url().strip_prefix(&config.server.prefix) {
        Some(url) => url.to_string(),
        None => {
            return Ok(not_found(format!(
                "Not found, try {}",
                config.server.prefix
            )))
        }
    };

    // For post requests, read the body. We need to do this once. The handler
    // may be retried, but the body we can only consume once.
    let mut body = String::new();
    if request.method() == &Method::Post {
        // Read the body, ignore any IO errors there. In most cases this is
        // probably fine and we'll fail elsewhere, but it might happen that
        // we read a truncated body and fail half-way.
        if let Err(_) = request.as_reader().read_to_string(&mut body) {
            return Ok(internal_error("Failed to read full request body."));
        }
    }

    with_transaction(connection, |tx| {
        if request.method() == &Method::Post {
            match url_inner.as_ref() {
                "/create-team" => endpoints::handle_create_team(config, tx, &user, &body),
                "/delete-team" => endpoints::handle_delete_team(config, tx, &user, &body),
                "/leave-team" => endpoints::handle_leave_team(config, tx, &user, &body),
                "/join-team" => endpoints::handle_join_team(config, tx, &user, &body),
                "/vote" => endpoints::handle_vote(config, tx, &user, &body),
                "/prev" => endpoints::handle_phase_prev(config, tx, &user),
                "/next" => endpoints::handle_phase_next(config, tx, &user),
                _ => Ok(not_found("Not found.")),
            }
        } else {
            // Assume everything else is a GET request.
            match url_inner.as_ref() {
                "" | "/" => endpoints::handle_index(config, tx, &user),
                _ => Ok(not_found("Not found.")),
            }
        }
    })
}

/// Run `f` in a transaction, retrying a few times if the database is busy.
///
/// SQLite does not support concurrent writes, but we do spawn multiple server
/// threads. It might happen that one of them encounters a concurrency error and
/// needs to restart the transaction, try that a few times before finally gving up.
fn with_transaction<F>(connection: &mut db::Connection, mut f: F) -> db::Result<Response>
where
    F: FnMut(&mut db::Transaction) -> db::Result<Response>,
{
    for attempt in 0.. {
        let mut tx = connection.begin()?;
        match f(&mut tx) {
            Ok(response) => {
                // Commit on success responses (we assume redirects to be success
                // as well, for example for use after submitting a form). If we
                // encounter any error, roll back. We do this here because
                // handlers cannot call `tx.rollback()`, because it consumes the
                // transaction.
                if response.status_code().0 < 400 {
                    tx.commit()?;
                } else {
                    tx.rollback()?;
                }
                return Ok(response);
            }
            Err(err) if err.code == Some(5) => {
                tx.rollback()?;
                println!("Database is locked (attempt {}): {err:?}", attempt + 1);
                // The database is locked by a writer. Retry if we haven't
                // retried too many times already.
                if attempt + 1 < 6 {
                    continue;
                } else {
                    return Ok(service_unavailable(
                        "The database is busy, wait a few seconds and try again.",
                    ));
                }
            }
            Err(err) => {
                // Try to roll back, but if it doesn't work, we are going to
                // open a new connection anyway.
                let _ = tx.rollback();
                return Err(err);
            }
        }
    }
    unreachable!("The number of continuations is bounded.");
}

fn serve_until_error(config: &Config, connection: &mut db::Connection, server: &Server) {
    loop {
        let mut fatal_error = None;
        let mut request = server.recv().unwrap();
        let start_time = Instant::now();

        let mut log_line = "Unparsed request".to_string();
        let response = match handle_request(config, connection, &mut request, &mut log_line) {
            Ok(resp) => {
                println!(
                    "{log_line} -> {} [{:.3} ms]",
                    resp.status_code().0,
                    (start_time.elapsed().as_micros() as f32) * 1e-3
                );
                resp
            }
            Err(err) => {
                // Some unrecoverable error happened.
                println!("{log_line} -> Error: {err:?}");
                fatal_error = Some(err);
                internal_error("Internal server error.")
            }
        };

        if let Err(err) = request.respond(response) {
            println!("Error writing response: {err:?}");
        }
        if let Some(err) = fatal_error {
            println!("Restarting server loop due to error: {err:?}");
            return;
        }
    }
}

fn main() {
    let config = Arc::new(load_config());

    let n_threads = config.server.num_threads as usize;
    let server = Arc::new(Server::http(&config.server.listen).unwrap());
    let mut guards = Vec::with_capacity(n_threads);
    let init_mutex = Arc::new(Mutex::new(()));

    // In theory everything should work with more server threads. And it does,
    // with 2 or 3, but with 4 or more threads, requests frequently get error 5
    // "database is locked" from SQLite. Printf debugging shows that all
    // transactions that get started also commit. But still, something is
    // holding on to the write lock? What's also really strange, it happens
    // frequently for 4 threads (~1 in 3 requests), while I haven't been able to
    // reproduce at all with 3 threads. But just to be sure, just do one.
    assert_eq!(n_threads, 1, "Currently only 1 thread works well.");

    for _ in 0..n_threads {
        let server = server.clone();
        let config = config.clone();
        let init_mutex = init_mutex.clone();

        let guard = thread::spawn(move || {
            loop {
                // The database connections need to be opened sequentially, because
                // SQLite supports only a single writer at a time. If we let all
                // threads run, then we encounter a "database is locked" error
                // (error code 5). We do need to open the connection on the server
                // threads though, we can't do it on the main thread because the
                // `db::Connection` takes a `&sqlite::Connection`, and the latter
                // is not `Sync`. So we have to initialize here. Setting the busy
                // timeout helps but is fragile: on an underpowered VM the timeout
                // may be insufficient. So mutexes it is.
                let db_lock = init_mutex.lock().unwrap();
                let raw_connection =
                    sqlite::open(&config.database.path).expect("Failed to open database");
                let mut connection =
                    init_database(&raw_connection).expect("Failed to initialize database.");
                std::mem::drop(db_lock);

                // Handle requests until we encounter a database error.
                // At that point we loop and open a fresh connection.
                serve_until_error(&config, &mut connection, &server);
            }
        });
        guards.push(guard);
    }

    println!(
        "Serving on http://{}{} ...",
        config.server.listen, config.server.prefix
    );

    for guard in guards.drain(..) {
        guard.join().unwrap();
    }
}
