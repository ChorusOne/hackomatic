use std::io::Cursor;
use std::str::FromStr;

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

fn main() {
    let config = parse_cli();

    let raw_connection = sqlite::open("hackomatic.sqlite").expect("Failed to open database");
    let mut connection = init_database(&raw_connection).expect("Failed to initialize database");

    let server = Server::http(&config.listen).unwrap();
    println!("Listening on http://{}/", config.listen);

    for mut request in server.incoming_requests() {
        let response = match handle_request(&config, &mut connection, &mut request) {
            Ok(response) => response,
            Err(err) => {
                println!("Error in response: {err:?}");
                Response::from_string("Internal server error".to_string()).with_status_code(500)
            }
        };
        match request.respond(response) {
            Err(err) => println!("Error: {err:?}"),
            Ok(()) => continue,
        }
    }
}
