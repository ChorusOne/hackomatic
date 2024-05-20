use std::io::Cursor;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

use tiny_http::{HeaderField, Request, Server};

use config::Config;
use database as db;

mod config;
mod database;
mod endpoints;

type Response = tiny_http::Response<Cursor<Vec<u8>>>;

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
    raw_connection.execute("PRAGMA busy_timeout = 30;")?;
    raw_connection.execute("PRAGMA journal_mode = WAL;")?;
    raw_connection.execute("PRAGMA foreign_keys = TRUE;")?;
    let mut connection = db::Connection::new(raw_connection);
    let mut tx = connection.begin()?;
    db::ensure_schema_exists(&mut tx)?;
    tx.commit()?;
    Ok(connection)
}

fn handle_request(
    config: &Config,
    connection: &mut db::Connection,
    request: &mut Request,
) -> db::Result<Response> {
    let mut tx = connection.begin()?;
    let response = handle_request_impl(config, &mut tx, request)?;
    tx.commit()?;
    Ok(response)
}

pub struct User<'a> {
    email: &'a str,
}

fn handle_request_impl(
    config: &Config,
    tx: &mut db::Transaction,
    request: &mut Request,
) -> db::Result<Response> {
    println!(
        "received request! method: {:?}, url: {:?}, headers: {:?}",
        request.method(),
        request.url(),
        request.headers()
    );

    // Figure out who the user is. In debug mode we fall back to a default.
    let header_x_email = HeaderField::from_str("X-Email").unwrap();
    let mut email = None;
    for header in request.headers() {
        if header.field == header_x_email {
            email = Some(header.value.as_str());
        }
    }
    let email = match email {
        Some(email) => email,
        None => match config.debug.unsafe_default_email.as_ref() {
            Some(fallback) => fallback,
            None => return Ok(Response::from_string("Missing authentication header.")),
        },
    };

    let user = User { email };

    endpoints::handle_index(tx, user)
}

fn serve_forever(config: &Config, connection: &mut db::Connection, server: &Server) -> ! {
    loop {
        let mut request = server.recv().unwrap();

        // SQLite does not support concurrent writes, but we do spawn multiple
        // server threads. It might happen that one of them encounters a
        // concurrency error and needs to restart the transaction, try that a
        // few times before finally gving up.
        let mut response =
            Response::from_string("Database is too busy".to_string()).with_status_code(503);
        for _attempt in 0..6 {
            match handle_request(config, connection, &mut request) {
                Ok(resp) => {
                    response = resp;
                    break;
                }
                Err(err) if err.code == Some(5) => {
                    // The database is locked by a writer. Retry.
                    continue;
                }
                Err(err) => {
                    // Some unrecoverable error happened.
                    println!("Error handling request: {err:?}");
                    response = Response::from_string("Internal server error".to_string())
                        .with_status_code(500);
                    break;
                }
            }
        }
        match request.respond(response) {
            Err(err) => println!("Error writing response: {err:?}"),
            Ok(()) => continue,
        }
    }
}

fn main() {
    let config = Arc::new(load_config());

    let n_threads = 4;
    let server = Arc::new(Server::http(&config.server.listen).unwrap());
    let mut guards = Vec::with_capacity(n_threads);

    for _ in 0..n_threads {
        let server = server.clone();
        let config = config.clone();
        let guard = thread::spawn(move || {
            let raw_connection =
                sqlite::open(&config.database.path).expect("Failed to open database");
            let mut connection =
                init_database(&raw_connection).expect("Failed to initialize database");
            serve_forever(&config, &mut connection, &server)
        });
        guards.push(guard);
    }

    println!("Listening on http://{}/ ...", config.server.listen);

    for guard in guards.drain(..) {
        guard.join().unwrap();
    }
}
