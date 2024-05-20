use crate::database as db;
use crate::{Response, User};

pub fn handle_index(tx: &mut db::Transaction, user: User) -> db::Result<Response> {
    let body = format!("Hello {}.", user.email);
    let result = Response::from_string(body);
    Ok(result)
}
