use tiny_http::{Server, Response};

fn main() {
    // TODO: Enable override.
    let server = Server::http("127.0.0.1:5591").unwrap();
    println!("Listening on http://127.0.0.1:5591/");

    for request in server.incoming_requests() {
        println!("received request! method: {:?}, url: {:?}, headers: {:#?}",
            request.method(),
            request.url(),
            request.headers()
        );

        let response = Response::from_string("hello world");
        let err = request.respond(response);
        if let Err(err) = err {
            println!("Error: {err:?}");
        }
    }
}
