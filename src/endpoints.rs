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

fn bad_request<R: Into<String>>(reason: R) -> Response {
    Response::from_string(reason.into()).with_status_code(400)
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

fn view_index(config: &Config, user: &User) -> Markup {
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
            p {
                details {
                    summary { "Add a new team" }
                    (form_create_team(config))
                }
            }
            h2 { "Vote" }
            p { "Voting has not commenced yet, check back later!" }
            h2 { "Winners" }
            p { "The winners will be announced here after the vote closes." }
        }
    }
}

fn form_create_team(config: &Config) -> Markup {
    let submit_url = format!("{}/create-team", config.server.prefix);
    html! {
        form action=(submit_url) method="post" {
            label {
                "Team name: ";
                input name="team-name";
            }
            input type="submit" value="Create Team";
        }
    }
}

pub fn handle_index(
    config: &Config,
    tx: &mut db::Transaction,
    user: &User,
) -> db::Result<Response> {
    let body = view_index(config, &user);
    Ok(respond_html(body))
}

/// Validate user inputs against a subset of Unicode.
///
/// Users should be able to input text, but allowing any Unicode code point
/// creates a can of worms where you can use distracting emoji, or reverse the
/// text direction for all following content, use the mathematical symbols to do
/// "markup", etc. So ban most of Unicode, but allow more than just ASCII
/// because Tomás and Mikołaj are valid non-ASCII names. This is very crude but
/// it'll do.
///
/// Returns the offending character on error.
fn is_string_sane(s: &str) -> Result<(), char> {
    for ch in s.chars() {
        // Control characters are not allowed (including newline).
        // Space (U+0020) is the first one that is allowed.
        if ch < '\u{20}' {
            return Err(ch);
        }

        // Allow General Punctuation (U+2000 through U+206F).
        if ch >= '\u{2000}' && ch < '\u{2070}' {
            continue;
        }

        // Allow Basic Latin, the supplement, extended Latin, modifiers,
        // diacritics, then a few other languages like Greek and Cyrillic, but
        // stop after Arabic.
        if ch >= '\u{0780}' {
            return Err(ch);
        }
    }

    Ok(())
}

pub fn handle_create_team(
    config: &Config,
    tx: &mut db::Transaction,
    user: &User,
    body: String,
) -> db::Result<Response> {
    let mut team_name = String::new();
    for (key, value) in form_urlencoded::parse(body.as_bytes()) {
        match key.as_ref() {
            "team-name" => team_name = value.trim().to_string(),
            _ => return Ok(bad_request("Unexpected form field, need 'team-name'.")),
        }
    }

    if team_name.is_empty() {
        return Ok(bad_request("The team name must not be empty."));
    }
    if team_name.len() > 100 {
        return Ok(bad_request("Team name may be no longer than 100 bytes."));
    }
    if let Err(ch) = is_string_sane(&team_name) {
        return Ok(bad_request(format!(
            "Invalid character in team name, '{ch}' (U+{:04X}) is not allowed.",
            ch as u32
        )));
    }

    Ok(respond_html(html! { "TODO" }))
}
