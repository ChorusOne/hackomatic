use std::str::FromStr;

use maud::{html, Markup, DOCTYPE};
use tiny_http::Header;

use crate::config::Config;
use crate::database as db;
use crate::{Phase, Response, User};

fn respond_html(markup: Markup) -> Response {
    Response::from_string(markup.into_string()).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
    )
}

fn respond_error<R: Into<String>>(reason: R) -> Response {
    let page = html! {
        (view_html_head("Hack-o-matic Error"))
        body {
            h1 { "D’oh!" }
            p { (reason.into()) }
        }
    };
    respond_html(page)
}

fn bad_request<R: Into<String>>(reason: R) -> Response {
    respond_error(reason).with_status_code(400)
}

fn conflict<R: Into<String>>(reason: R) -> Response {
    respond_error(reason).with_status_code(409)
}

fn forbidden<R: Into<String>>(reason: R) -> Response {
    respond_error(reason).with_status_code(403)
}

fn redirect_see_other<R: AsRef<[u8]>>(location: R) -> Response {
    Response::from_string("")
        .with_status_code(303)
        .with_header(Header::from_bytes(&b"Location"[..], location.as_ref()).unwrap())
}

/// Render the standard header that is the same across all pages.
fn view_html_head(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        head {
            meta charset="utf-8";
            link rel="preconnect" href="https://fonts.googleapis.com";
            link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
            link href="https://fonts.googleapis.com/css2?family=Work+Sans:ital,wght@0,700..800;1,900&family=Atkinson+Hyperlegible:ital,wght@0,400;0,700;1,400&display=swap" rel="stylesheet";
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

fn view_email<'a>(config: &Config, email: &'a str) -> &'a str {
    match email.strip_suffix(&config.app.email_suffix) {
        Some(stripped) => stripped,
        None => email,
    }
}

fn view_index(
    config: &Config,
    user: &User,
    phase: Phase,
    teams: &[(db::Team, Vec<String>)],
) -> Markup {
    html! {
        (view_html_head("Hack-o-matic"))
        body {
            h1 {
                "Hack-o-matic"
            }
            p {
                "Welcome to the hackaton support system, " (user.email) "."
            }
            (view_phases(phase))
            @if user.is_admin {
                (view_phase_admin_form(config))
            }
            @if matches!(phase, Phase::Evaluation) {
                (view_voting_help(config))
            }
            h2 { "Teams" }
            @if matches!(phase, Phase::Registration) {
                p {
                    details {
                        summary { "Add a new team" }
                        (form_create_team(config))
                    }
                }
            }
            @for team in teams {
                // We give teams an anchor so we can refer to it from a
                // redirect and even highlight after creation using CSS.
                div .team id=(format!("team-{}", team.0.id)) {
                    h3 {
                        a href=(format!("{}#team-{}", config.server.prefix, team.0.id)) {
                            (team.0.name)
                        }
                    }
                    p .description { (team.0.description) }
                    p .members {
                        strong { "Members: " }
                        @for (i, member) in team.1.iter().enumerate() {
                            @if i > 0 { ", " }
                            (view_email(config, member))
                        }
                    }
                    @if matches!(phase, Phase::Registration) {
                        (form_team_actions(config, user, team.0.id, &team.1))
                    }
                }
            }
        }
    }
}

fn view_phase_admin_form(config: &Config) -> Markup {
    let submit_next = format!("{}/next", config.server.prefix);
    let submit_prev = format!("{}/prev", config.server.prefix);
    html! {
        form method="post" {
            button type="submit" formaction=(submit_prev) { "Restore Previous Phase" }
            " "
            button type="submit" formaction=(submit_next) { "Start Next Phase" }
        }
    }
}

fn view_phases(current: Phase) -> Markup {
    let here = html! {
        " " div .here { "We are here" }
    };

    html! {
        h2 { "Progress" }
        p { "The hackathon proceeds in five steps:" }
        ol {
            li {
                strong { "Registration" }
                " — Participants form teams."
                @if matches!(current, Phase::Registration) { (here) }
            }
            li {
                strong { "Presentation" }
                " — Teams present what they built."
                @if matches!(current, Phase::Presentation) { (here) }
            }
            li {
                strong { "Evaluation" }
                " — Everybody votes for their favorite teams."
                @if matches!(current, Phase::Evaluation) { (here) }
            }
            li {
                strong { "Revelation" }
                " — We announce the winners."
                @if matches!(current, Phase::Revelation) { (here) }
            }
            li {
                strong { "Celebration" }
                " — The end of the hackathon."
                @if matches!(current, Phase::Celebration) { (here) }
            }
        }
    }
}

fn view_voting_help(config: &Config) -> Markup {
    html! {
        h2 { "Voting" }
        p {
            "Voting is now open. We are using "
            em { "quadratic voting" } ". "
            "It works as follows:"
        }
        ol {
            li { "You get " (config.app.coins_to_spend) " " em { "coins" } "." }
            li { "You can spend coins to give teams " em { "points" } "." }
            li { "The cost in coins is the square of the points you award per team." }
        }
        p {
            "This means that if you " em { "really" } " like one team, "
            "you can spend all your coins on them, "
            "but you can award more points in total "
            "by distributing your votes across multiple teams. "
            "For example, here are some ways to spend 50 coins, "
            "with the points in bold and the cost in parentheses:"
        }
        ul {
            li {
                strong { "7" } " (49), "
                strong { "1" } " (1)"
            }
            li {
                strong { "6" } " (36), "
                strong { "3" } " (9), "
                strong { "2" } " (4), "
                strong { "1" } " (1)"
            }
            li {
                strong { "5" } " (25), "
                strong { "5" } " (25)"
            }
            li {
                strong { "4" } " (16), "
                strong { "4" } " (16), "
                strong { "4" } " (16), "
                strong { "1" } " (1), "
                strong { "1" } " (1)"
            }
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
            label {
                "One-line description: ";
                input name="description";
            }
            button type="submit" { "Create Team" }
        }
    }
}

fn form_team_actions(config: &Config, user: &User, team_id: i64, members: &[String]) -> Markup {
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
    let phase = crate::load_phase(tx)?;
    let teams = db::iter_teams(tx)?.collect::<Result<Vec<_>, _>>()?;
    let mut teams_with_members = Vec::with_capacity(teams.len());
    for team in teams {
        let members = db::iter_team_members(tx, team.id)?.collect::<Result<Vec<_>, _>>()?;
        teams_with_members.push((team, members));
    }

    let body = view_index(config, &user, phase, &teams_with_members);
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
        Err(err)
            if err
                .message
                .as_deref()
                .unwrap_or("")
                .contains("UNIQUE constraint") =>
        {
            return Ok(bad_request("A team with that name already exists."))
        }
        Err(err) => return Err(err),
    };

    // The user who creates the team is initially a member of it.
    db::add_team_member(tx, team_id, &user.email)?;

    let new_url = format!("{}#team-{}", config.server.prefix, team_id);
    Ok(redirect_see_other(new_url.as_bytes()))
}

fn get_body_team_id(body: String) -> Result<i64, Response> {
    let mut team_id = 0_i64;

    for (key, value) in form_urlencoded::parse(body.as_bytes()) {
        match key.as_ref() {
            "team-id" => match i64::from_str(value.as_ref()) {
                Ok(id) => team_id = id,
                Err(..) => return Err(bad_request("Invalid team id.")),
            },
            _ => return Err(bad_request("Unexpected form field.")),
        }
    }

    if team_id == 0 {
        Err(bad_request("Need a team id."))
    } else {
        Ok(team_id)
    }
}

pub fn handle_delete_team(
    config: &Config,
    tx: &mut db::Transaction,
    user: &User,
    body: String,
) -> db::Result<Response> {
    let team_id = match get_body_team_id(body) {
        Ok(id) => id,
        Err(err_response) => return Ok(err_response),
    };

    // Remove ourselves from the team first.
    db::remove_team_member(tx, team_id, &user.email)?;

    // Confirm that the team is now empty.
    for _member in db::iter_team_members(tx, team_id)? {
        // Returning an error status code will also roll back the transaction.
        return Ok(conflict("The team is not empty, we can't delete it yet."));
    }

    db::delete_team(tx, team_id)?;

    Ok(redirect_see_other(config.server.prefix.as_bytes()))
}

pub fn handle_leave_team(
    config: &Config,
    tx: &mut db::Transaction,
    user: &User,
    body: String,
) -> db::Result<Response> {
    let team_id = match get_body_team_id(body) {
        Ok(id) => id,
        Err(err_response) => return Ok(err_response),
    };

    // Remove ourselves from the team first.
    db::remove_team_member(tx, team_id, &user.email)?;

    // Confirm that the team is not empty. If it is, we should have deleted it.
    // We could do it automatically but let's be safe and not delete anything
    // unless a delete is explicitly what was requested.
    if db::iter_team_members(tx, team_id)?.next().is_none() {
        return Ok(conflict(
            "It looks like all your team members have abandoned you.\n\
            You are the last member, leaving the team would leave it empty.\n\
            If you really want to do that to the team, then go back, \n\
            refresh the page, and choose 'Delete Team'.",
        ));
    }

    let new_url = format!("{}#team-{}", config.server.prefix, team_id);
    Ok(redirect_see_other(new_url.as_bytes()))
}

pub fn handle_join_team(
    config: &Config,
    tx: &mut db::Transaction,
    user: &User,
    body: String,
) -> db::Result<Response> {
    let team_id = match get_body_team_id(body) {
        Ok(id) => id,
        Err(err_response) => return Ok(err_response),
    };

    // Confirm that the team exists before we join it. For it to exist, it must
    // have members.
    if db::iter_team_members(tx, team_id)?.next().is_none() {
        return Ok(conflict(
            "It looks like all team members have left this team before you joined.\n\
            It no longer exists, but if you like you can go back and create a new team.",
        ));
    }

    db::add_team_member(tx, team_id, &user.email)?;

    let new_url = format!("{}#team-{}", config.server.prefix, team_id);
    Ok(redirect_see_other(new_url.as_bytes()))
}

pub fn handle_phase_prev(
    config: &Config,
    tx: &mut db::Transaction,
    user: &User,
) -> db::Result<Response> {
    if !user.is_admin {
        return Ok(forbidden("Only the admin is allowed to change the phase."));
    }
    let current = crate::load_phase(tx)?;
    db::set_current_phase(tx, current.prev().to_str())?;
    Ok(redirect_see_other(config.server.prefix.as_bytes()))
}

pub fn handle_phase_next(
    config: &Config,
    tx: &mut db::Transaction,
    user: &User,
) -> db::Result<Response> {
    if !user.is_admin {
        return Ok(forbidden("Only the admin is allowed to change the phase."));
    }
    let current = crate::load_phase(tx)?;
    db::set_current_phase(tx, current.next().to_str())?;
    Ok(redirect_see_other(config.server.prefix.as_bytes()))
}
