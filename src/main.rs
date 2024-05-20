use std::io::Cursor;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

use tiny_http::{HeaderField, Request, Server};

use database as db;

mod database;

type Response = tiny_http::Response<Cursor<Vec<u8>>>;

struct Config {
    is_debug: bool,
    listen: String,
}

fn parse_cli() -> Config {
    let mut result = Config {
        is_debug: false,
        listen: "127.0.0.1:5591".to_string(),
    };

    let mut args = std::env::args();

    // Skip the program name.
    args.next();

    for arg in args {
        match arg.as_ref() {
            "--debug" => result.is_debug = true,
            _ => result.listen = arg,
        }
    }

    result
}

fn init_database(raw_connection: &sqlite::Connection) -> db::Result<db::Connection> {
    // Change the database to WAL mode if it wasn't already. Set the busy
    // timeout to 30 milliseconds, so readers and writers can wait for each
    // other a little bit. We also have a retry loop around the request handler.
    raw_connection.execute("PRAGMA busy_timeout = 30;")?;
    raw_connection.execute("PRAGMA journal_mode = WAL;")?;
    raw_connection.execute("PRAGMA foreign_keys = TRUE;")?;
    let mut connection = db::Connection::new(&raw_connection);
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
        None if config.is_debug => "debug@example.com",
        _ => return Ok(Response::from_string("Missing authentication header.")),
    };

    let body = format!("Hello {email}.");
    let result = Response::from_string(body);
    Ok(result)
}

fn serve_forever(
    config: &Config,
    connection: &mut db::Connection,
    server: &Server,
) -> ! {
    loop {
        let mut request = server.recv().unwrap();

        // SQLite does not support concurrent writes, but we do spawn multiple
        // server threads. It might happen that one of them encounters a
        // concurrency error and needs to restart the transaction, try that a
        // few times before finally gving up.
        let mut response = Response::from_string("Database is too busy".to_string()).with_status_code(503);
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
                    response = Response::from_string("Internal server error".to_string()).with_status_code(500);
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
    let config = Arc::new(parse_cli());

    let n_threads = 4;
    let server = Arc::new(Server::http(&config.listen).unwrap());
    let mut guards = Vec::with_capacity(n_threads);

    for _ in 0..n_threads {
        let server = server.clone();
        let config = config.clone();
        let guard = thread::spawn(move || {
            let raw_connection = sqlite::open("hackomatic.sqlite").expect("Failed to open database");
            let mut connection = init_database(&raw_connection).expect("Failed to initialize database");
            serve_forever(&config, &mut connection, &server)
        });
        guards.push(guard);
    }

    println!("Listening on http://{}/", config.listen);

    for guard in guards.drain(..) {
        guard.join().unwrap();
    }
}
