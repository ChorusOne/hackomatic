use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::Hasher;
use std::str::FromStr;

use maud::{html, Markup, DOCTYPE};
use tiny_http::Header;

use crate::config::Config;
use crate::database as db;
use crate::{Phase, Response, User};

/// Additional data to display with the team, depending on the phase.
enum TeamData {
    None,
    /// The points that the current user awarded to this team.
    UserVote {
        points: i64,
    },
    /// All the votes for this team.
    AllVotes {
        votes: Vec<db::Vote>,
    },
}

struct TeamEntry {
    team: db::Team,
    member_emails: Vec<String>,
    data: TeamData,
    total_points: i64,
    rank: u32,
}

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

// Same for the script.
#[cfg(debug_assertions)]
fn get_vote_script() -> Markup {
    let data =
        std::fs::read_to_string("src/vote.js").expect("Need to run from repo root in debug mode.");
    maud::PreEscaped(data)
}

#[cfg(not(debug_assertions))]
fn get_vote_script() -> Markup {
    maud::PreEscaped(include_str!("vote.js").to_string())
}

fn view_email<'a>(config: &Config, email: &'a str) -> &'a str {
    match email.strip_suffix(&config.app.email_suffix) {
        Some(stripped) => stripped,
        None => email,
    }
}

struct IndexData<'a> {
    phase: Phase,
    teams: &'a [TeamEntry],
    cheaters: &'a [String],
    voter_count: u32,
}

fn view_index(config: &Config, user: &User, data: IndexData) -> Markup {
    // During the vote phase, we tweak the message depending on whether the user
    // has voted or not, so we need to know if they have any.
    let mut did_vote = false;
    for team in data.teams {
        match team.data {
            TeamData::UserVote { points } if points != 0 => {
                did_vote = true;
                break;
            }
            _ => continue,
        }
    }

    html! {
        (view_html_head("Hack-o-matic"))
        body {
            h1 {
                "Hack-o-matic"
            }
            p {
                "Welcome to the hackaton support system, " (user.email) "."
            }
            (view_phases(data.phase))
            @if user.is_admin {
                (view_phase_admin_form(config, data.phase))
            }
            @if matches!(data.phase, Phase::Evaluation | Phase::Revelation | Phase::Celebration) {
                h2 { "Voting Turnout" }
                p {
                    @match data.voter_count {
                        0 => "Nobody has cast a vote yet.",
                        1 => "One person has cast their vote.",
                        n => { (n) " people have cast their vote." },
                    }
                }
            }
            @if matches!(data.phase, Phase::Revelation | Phase::Celebration) {
                // Display the cheaters only after the vote ends. That way
                // people are not immediately discouraged from cheating during
                // the vote when the first person tries to do it. I'm curious to
                // catch all the people who would try.
                @if !data.cheaters.is_empty() {
                    h2 { "Hall of Shame" }
                    p { "The following people tried to cheat and vote for themselves:" }
                    ul {
                        @for cheater_email in data.cheaters {
                            li { (view_email(config, &cheater_email)) }
                        }
                    }
                }
            }
            @if matches!(data.phase, Phase::Evaluation) {
                (view_voting_help(config))
            }
            h2 { "Teams" }
            @if matches!(data.phase, Phase::Registration) {
                p {
                    details {
                        summary { "Add a new team" }
                        (form_create_team(config))
                    }
                }
            }
            @if matches!(data.phase, Phase::Evaluation) {
                form
                    action=(format!("{}/vote", config.server.prefix))
                    method="post"
                {
                    @for entry in data.teams {
                        (view_team(config, user, data.phase, entry))
                    }
                    div .score-float {
                        h2 { "Your Vote" }
                        p {
                            "You have "
                            span #coins-left {
                                (config.app.coins_to_spend) " coins"
                            }
                            " left to spend. "
                            noscript {
                                "If you enable Javascript, "
                                "this number updates as you edit your vote, "
                                "but now it only shows the total you can spend."
                            }
                        }
                    }
                    footer {
                        button type="submit" #submit-vote { "Submit Vote" }
                        p #voteMessage {
                            @if did_vote {
                                "Your vote has been recorded. "
                                "You can still change it as long as voting is open."
                            } @else {
                                "You have not voted yet. "
                                "You can still change your vote after you submit, "
                                "as long as voting is open."
                            }
                        }
                    }
                    // This is here as anchor so that after submit we are at the
                    // end of the page.
                    div #your-vote {}
                }
                script {
                    "const coinsToSpend = " (config.app.coins_to_spend) ";\n"
                    "const inputBoxes = [";
                    @for entry in data.teams { "input" (entry.team.id) ", " }
                    "];\n"
                    (get_vote_script())
                }
            } @else {
                @for entry in data.teams {
                    (view_team(config, user, data.phase, entry))
                }
            }
        }
    }
}

fn view_team(config: &Config, user: &User, phase: Phase, entry: &TeamEntry) -> Markup {
    // Due to quadratic cost, the max points you can spend is the square root
    // of the coins you have.
    let max_points = (config.app.coins_to_spend as f32).sqrt().floor() as i32;
    let user_points = match entry.data {
        TeamData::UserVote { points } => points,
        _ => 0,
    };
    let supporters = match &entry.data {
        TeamData::AllVotes { votes } => Some(&votes[..]),
        _ => None,
    };

    html! {
        // We give teams an anchor so we can refer to it from a
        // redirect and even highlight after creation using CSS.
        div .team id=(format!("team-{}", entry.team.id)) {
            @if matches!(&entry.data, TeamData::AllVotes { .. }) {
                div .outcome-outer {
                    div .outcome {
                        div .rank { (entry.rank) }
                        div .points {
                            @match entry.total_points {
                                0 => "0 points",
                                1 => "1 point",
                                n => { (n) " points" },
                            }
                        }
                    }
                }
            }
            h3 {
                a href=(format!("{}#team-{}", config.server.prefix, entry.team.id)) {
                    (entry.team.name)
                }
            }
            p .description { (entry.team.description) }
            p {
                strong { "Members: " }
                @for (i, member) in entry.member_emails.iter().enumerate() {
                    @if i > 0 { ", " }
                    (view_email(config, member))
                }
                @if let Some(supporters) = supporters {
                    @if !supporters.is_empty() {
                        br;
                        strong { "Supporters: " }
                        @for (i, vote) in supporters.iter().enumerate() {
                            @if i > 0 { ",\u{2002}" }
                            (view_email(config, &vote.voter_email))
                            " (" (vote.points) ")"
                        }
                    }
                }
            }
            @if matches!(phase, Phase::Registration) {
                (form_team_actions(config, user, entry.team.id, &entry.member_emails))
            }
            @if matches!(phase, Phase::Evaluation) {
                label {
                    "Your points: ";
                    @if entry.member_emails.contains(&user.email) {
                        input
                            id=(format!("input{}", entry.team.id))
                            name=(format!("team-{}", entry.team.id))
                            disabled
                            value=(user_points)
                            title="You can’t vote for this team because you are a member.";
                    } @else {
                        input
                            id=(format!("input{}", entry.team.id))
                            name=(format!("team-{}", entry.team.id))
                            type="number"
                            min="0"
                            max=(max_points)
                            value=(user_points);
                    }
                    // Add a span where js will put the computed cost of this
                    // vote. Don't bother rendering it server side too, we'll
                    // just run the js after page load, and if you view the page
                    // with js disabled, it's better to not show anything than
                    // to show the wrong number.
                    span .cost id=(format!("cost{}", entry.team.id));
                }
            }
        }
    }
}

fn view_phase_admin_form(config: &Config, phase: Phase) -> Markup {
    let submit_next = format!("{}/next", config.server.prefix);
    let submit_prev = format!("{}/prev", config.server.prefix);
    let can_next = phase.next() != phase;
    let can_prev = phase.prev() != phase;
    html! {
        form method="post" {
            button
                type="submit"
                formaction=(submit_prev)
                disabled[!can_prev]
            { "← Previous Phase" }
            " "
            button
                type="submit"
                formaction=(submit_next)
                disabled[!can_next]
            { "Next Phase →" }
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
        h2 { "Voting System" }
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
            "For example, here are some ways to spend 100 coins, "
            "with the points in bold and the cost per team in parentheses:"
        }
        ul {
            li {
                "1 × " strong { "10" } " (100) "
            }
            li {
                "2 × " strong { "7" } " (49),\u{2002}"
                "2 × " strong { "1" } " (1)"
            }
            li {
                "2 × " strong { "6" } " (36),\u{2002}"
                "1 × " strong { "5" } " (25),\u{2002}"
                "3 × " strong { "1" } " (1)"
            }
            li {
                "4 × " strong { "5" } " (25)"
            }
            li {
                "6 × " strong { "4" } " (16),\u{2002}"
                "1 × " strong { "2" } " (4)"
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
    let mut team_entries = Vec::with_capacity(teams.len());

    for team in teams {
        let mut total_points = 0;
        let members = db::iter_team_members(tx, team.id)?.collect::<Result<Vec<_>, _>>()?;
        let data = match phase {
            Phase::Registration | Phase::Presentation => TeamData::None,
            Phase::Evaluation => {
                let points = db::get_team_vote_for(tx, team.id, &user.email)?;
                TeamData::UserVote {
                    points: points.unwrap_or(0),
                }
            }
            Phase::Revelation | Phase::Celebration => {
                if user.can_see_outcome(phase) {
                    let votes = db::iter_team_votes(tx, team.id)?.collect::<Result<Vec<_>, _>>()?;
                    // The votes have been validated, so this should not
                    // overflow unless we have a crazy number of voters.
                    total_points = votes.iter().map(|v| v.points).sum();
                    TeamData::AllVotes { votes: votes }
                } else {
                    TeamData::None
                }
            }
        };

        let entry = TeamEntry {
            team,
            data,
            total_points,
            member_emails: members,
            rank: 0,
        };
        team_entries.push(entry);
    }

    // During the voting phase, shuffle all teams randomly for every user. This
    // is to try and eliminate (or at least average out) bias for voting for the
    // first or last entry. The sort key is pseudorandom but deterministic.
    // Every user gets a different order, but for that user, the order is the
    // same on every page load, even across restarts of the binary.
    if matches!(phase, Phase::Evaluation) {
        for entry in team_entries.iter_mut() {
            let mut hasher = DefaultHasher::new();
            hasher.write(user.email.as_bytes());
            hasher.write_i64(entry.team.id);
            // We abuse the total points field to store the sort key in,
            // it's not used during the voting phase anyway.
            entry.total_points = hasher.finish() as i64;
        }
        team_entries.sort_by_key(|entry| entry.total_points);
    }

    // If we are displaying points, sort and compute the rank.
    if user.can_see_outcome(phase) {
        team_entries.sort_by_key(|entry| (-entry.total_points, entry.team.id));
        let mut rank = 0;
        let mut prev_points = -1;
        for entry in team_entries.iter_mut() {
            // Teams that have the same number of points have the same rank.
            // I briefly considered breaking ties by the number of voters, but
            // that would kind of defeat the purpose of quadratic voting, so
            // let's keep it at points only.
            if entry.total_points != prev_points {
                rank += 1;
                prev_points = entry.total_points;
            }
            entry.rank = rank;
        }

        // Normally you want to see the teams from first to last. But during the
        // revelation ceremony, when the admin loads the page, it is very
        // convenient if the lowest ranked teams are at the top, then you can
        // just scroll down to reveal the next team.
        if matches!(phase, Phase::Revelation) {
            team_entries.reverse();
        }
    }

    let cheaters = db::iter_cheaters(tx)?.collect::<Result<Vec<_>, _>>()?;
    let voter_count = db::count_voters(tx)?;

    let data = IndexData {
        phase,
        teams: &team_entries,
        cheaters: &cheaters,
        voter_count: voter_count as u32,
    };

    let body = view_index(config, &user, data);
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

/// Sum the squares of the values in the hashmap without overflow.
fn get_coins_spent<T>(points: &HashMap<T, i64>) -> Option<i64> {
    let mut total: i64 = 0;
    for p in points.values() {
        let p2 = p.checked_mul(*p)?;
        total = total.checked_add(p2)?;
    }
    Some(total)
}

pub fn handle_vote(
    config: &Config,
    tx: &mut db::Transaction,
    user: &User,
    body: String,
) -> db::Result<Response> {
    // Map team id to points. Would be nice to do a newtype wrapper for teams
    // but I can't be bothered right now.
    let mut teams_points: HashMap<i64, i64> = HashMap::new();

    for (key, value) in form_urlencoded::parse(body.as_bytes()) {
        match key.as_ref().strip_prefix("team-") {
            Some(team_id_str) => {
                match (i64::from_str(team_id_str), i64::from_str(value.as_ref())) {
                    (Ok(team_id), Ok(points)) => {
                        teams_points.insert(team_id, points);
                    }
                    (_, Err(..)) => {
                        return Ok(bad_request("You need to enter a number for every team."))
                    }
                    (Err(..), _) => return Ok(bad_request("Invalid team id.")),
                }
            }
            None => return Ok(bad_request("Unexpected form field.")),
        }
    }

    // Verify that the user is not spending more coins than allowed.
    let coins_spent = match get_coins_spent(&teams_points) {
        Some(t) => t,
        None => return Ok(bad_request("Overflowing an i64? Nice try, but no.")),
    };
    if coins_spent > config.app.coins_to_spend as i64 {
        return Ok(bad_request(format!(
            "You tried to spend {} coins, but you can spend at most {}.",
            coins_spent, config.app.coins_to_spend,
        )));
    }

    // If the user tries to vote for a team that they're a member of, reset back
    // to zero and add them to the hall of shame.
    let mut did_cheat = false;
    for team_id_opt in db::iter_member_teams(tx, &user.email)? {
        if let Some(p) = teams_points.get_mut(&team_id_opt?) {
            if *p != 0 {
                *p = 0;
                did_cheat = true;
            }
        }
    }
    if did_cheat {
        db::set_cheater(tx, &user.email)?;
    }

    // Clear out any old votes, in case the user already voted previously.
    db::delete_votes_for_voter(tx, &user.email)?;

    for (team_id, points) in teams_points.iter() {
        if *points == 0 {
            // No need to pollute the database with zero votes that don't do
            // anything.
            continue;
        }
        if *points < 0 {
            return Ok(bad_request(
                "While the math works fine if we allow awarding negative points, \
                are you really that kind of person who chooses to spend their coins \
                on destroying somebody else’s reputation, \
                rather than just voting for a different team?",
            ));
        }
        db::insert_vote(tx, &user.email, *team_id, *points)?;
    }

    let new_url = format!("{}#your-vote", config.server.prefix);
    Ok(redirect_see_other(new_url.as_bytes()))
}
