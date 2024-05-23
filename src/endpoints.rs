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

fn view_index(config: &Config, user: &User, teams: &[(db::Team, Vec<String>)]) -> Markup {
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
            p {
                details {
                    summary { "Add a new team" }
                    (form_create_team(config))
                }
            }
            @for team in teams {
                // We give teams an anchor so we can refer to it from a
                // redirect and even highlight after creation using CSS.
                h3 id=(format!("team-{}", team.0.id)) {
                    a href=(format!("{}#team-{}", config.server.prefix, team.0.id)) {
                        (team.0.name)
                    }
                }
                p .description { (team.0.description) }
                p .members {
                    @if team.1.is_empty() {
                        // TODO: Should I just delete it after the last person leaves?
                        "All members have left this team."
                    } else {
                        "Members: "
                        (team.1.join(", "))
                    }
                }
                (form_team_actions(config, user, team.0.id, &team.1))
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
            br;
            label {
                "One-line description: ";
                input name="description";
            }
            br;
            button type="submit" { "Create Team" }
        }
    }
}

fn form_team_actions(
    config: &Config,
    user: &User,
    team_id: i64,
    members: &[String],
) -> Markup {
    // Linear search, I know I know. Teams are small anyway.
    let is_member = members.contains(&user.email);
    let is_singleton = members.len() == 1;

    let (slug, label) = if is_member && is_singleton {
        ("delete-team", "Delete Team")
    } else if is_member {
        ("leave-team", "Leave Team")
    } else {
        ("join-team", "Join Team")
    };

    let submit_url = format!("{}/{}", config.server.prefix, slug);
    html! {
        form action=(submit_url) method="post" {
            input type="hidden" name="team-id" value=(team_id);
            button type="submit" { (label) }
        }
    }
}

pub fn handle_index(
    config: &Config,
    tx: &mut db::Transaction,
    user: &User,
) -> db::Result<Response> {
    let teams = db::iter_teams(tx)?.collect::<Result<Vec<_>, _>>()?;
    let mut teams_with_members = Vec::with_capacity(teams.len());
    for team in teams {
        let members = db::iter_team_members(tx, team.id)?.collect::<Result<Vec<_>, _>>()?;
        teams_with_members.push((team, members));
    }

    let body = view_index(config, &user, &teams_with_members);
    Ok(respond_html(body))
}

/// Validate user inputs against length limits and Unicode subset.
///
/// Users should be able to input text, but allowing any Unicode code point
/// creates a can of worms where you can use distracting emoji, or reverse the
/// text direction for all following content, use the mathematical symbols to do
/// "markup", etc. So ban most of Unicode, but allow more than just ASCII
/// because Tomás and Mikołaj are valid non-ASCII names. This is very crude but
/// it'll do.
///
/// Returns the offending character on error.
fn validate_string(label: &'static str, max_len: usize, input: &str) -> Result<(), String> {
    if input.is_empty() {
        return Err(format!("{label} must not be empty."));
    }

    if input.len() > max_len {
        return Err(format!("{label} may not be longer than {max_len} bytes."));
    }

    for ch in input.chars() {
        // Control characters are not allowed (including newline).
        // Space (U+0020) is the first one that is allowed.
        if ch < '\u{20}' {
            return Err(format!(
                "{label} may not contain control characters (including newlines)."
            ));
        }

        // Allow General Punctuation (U+2000 through U+206F).
        if ch >= '\u{2000}' && ch < '\u{2070}' {
            continue;
        }

        // Allow Basic Latin, the supplement, extended Latin, modifiers,
        // diacritics, then a few other languages like Greek and Cyrillic, but
        // stop after Arabic.
        if ch >= '\u{0780}' {
            return Err(format!(
                "{label} contains an invalid character: ‘{ch}’ (U+{:04X}) is not allowed.",
                ch as u32
            ));
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
    let mut description = String::new();

    for (key, value) in form_urlencoded::parse(body.as_bytes()) {
        match key.as_ref() {
            "team-name" => team_name = value.trim().to_string(),
            "description" => description = value.trim().to_string(),
            _ => return Ok(bad_request("Unexpected form field.")),
        }
    }

    if let Err(msg) = validate_string("The team name", 65, &team_name) {
        return Ok(bad_request(msg));
    }
    if let Err(msg) = validate_string("The description", 120, &description) {
        return Ok(bad_request(msg));
    }

    let n_teams_by_user = db::count_teams_by_creator(tx, &user.email)?;
    if n_teams_by_user > config.app.max_teams_per_creator as i64 {
        return Ok(bad_request(format!(
            "You already created {n_teams_by_user} teams, chill out!"
        )));
    }

    let team_id = match db::add_team(tx, &team_name, &user.email, &description) {
        Ok(id) => id,
        Err(err) if err.message.as_deref().unwrap_or("").contains("UNIQUE constraint") => {
            return Ok(bad_request("A team with that name already exists."))
        }
        Err(err) => return Err(err),
    };

    // The user who creates the team is initially a member of it.
    db::add_team_member(tx, team_id, &user.email)?;

    let new_url = format!("{}#team-{}", config.server.prefix, team_id);

    let result = Response::from_string("")
        .with_status_code(303)
        .with_header(Header::from_bytes(&b"Location"[..], new_url.as_bytes()).unwrap());

    Ok(result)
}
