use maud::{html, Markup, DOCTYPE};

use tiny_http::Header;

use crate::config::Config;
use crate::database as db;
use crate::{Response, User};

fn respond_html(markup: Markup) -> Response {
    Response::from_string(markup.into_string()).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
    )
}

/// Render the standard header that is the same across all pages.
fn view_html_head(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        head {
            meta charset="utf-8";
            link rel="preconnect" href="https://rsms.me/";
            link rel="stylesheet" href="https://rsms.me/inter/inter.css";
            link rel="preconnect" href="https://fonts.googleapis.com";
            link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
            link href="https://fonts.googleapis.com/css2?family=Work+Sans:ital,wght@0,100..900;1,100..900&display=swap" rel="stylesheet";
            meta name="viewport" content="width=device-width, initial-scale=1";
            title { (page_title) }
            style { (get_stylesheet()) }
        }
    }
}

// In debug mode, we load the stylesheet from disk on the fly, so you can edit
// without having to rebuild the server.
#[cfg(debug_assertions)]
fn get_stylesheet() -> Markup {
    let data = std::fs::read_to_string("src/style.css")
        .expect("Need to run from repo root in debug mode.");
    html! { (data) }
}

// For a release build, we embed the stylesheet into the binary.
#[cfg(not(debug_assertions))]
fn get_stylesheet() -> Markup {
    let data = include_str!("style.css");
    html! { (data) }
}

fn view_index(user: User) -> Markup {
    html! {
        (view_html_head("Hack-o-matic"))
        body {
            h1 {
                "Hack-o-matic"
            }
            p {
                "Welcome to the hackaton support system, " (user.email) "."
            }
            h2 { "Teams" }
            p { "Meet the contestants!" }
            (view_create_team())
            h2 { "Vote" }
            p { "Voting has not commenced yet, check back later!" }
            h2 { "Winners" }
            p { "The winners will be announced here after the vote closes." }
        }
    }
}

fn view_create_team() -> Markup {
    html! {
        form action="hack/create-team" method="post" {
            label {
                "Team name: ";
                input name="team-name";
            }
            input type="submit" value="Create Team";
        }
    }
}

pub fn handle_index(config: &Config, tx: &mut db::Transaction, user: User) -> db::Result<Response> {
    let body = view_index(user);
    Ok(respond_html(body))
}

pub fn handle_create_team(config: &Config, tx: &mut db::Transaction, user: User) -> db::Result<Response> {
    Ok(respond_html(html! { "TODO" }))
}
