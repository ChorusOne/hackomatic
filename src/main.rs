use std::io::Cursor;
use std::str::FromStr;

use tiny_http::{HeaderField, Request, Server};

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

fn handle_request(config: &Config, request: &mut Request) -> Response {
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
        _ => return Response::from_string("Missing authentication header."),
    };

    let body = format!("Hello {email}.");
    Response::from_string(body)
}

fn main() {
    let config = parse_cli();

    let server = Server::http(&config.listen).unwrap();
    println!("Listening on http://{}/", config.listen);

    for mut request in server.incoming_requests() {
        let response = handle_request(&config, &mut request);
        match request.respond(response) {
            Err(err) => println!("Error: {err:?}"),
            Ok(()) => continue,
        }
    }
}
