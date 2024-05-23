use serde::{self, Deserialize};

/// Application configuration.
///
/// The configuration is trivial, but split into structs anyway to make the
/// structure of the corresponding toml file a bit nicer.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub app: AppConfig,
    #[serde(default)]
    pub debug: DebugConfig,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    /// The suffix to remove from user emails when listing them.
    pub email_suffix: String,

    /// The maximum number of teams that a user can create.
    pub max_teams_per_creator: u32,
}

#[derive(Debug, Default, Deserialize)]
pub struct DebugConfig {
    /// Use this as fallback email when the `X-Email` header is not set.
    ///
    /// In a production deployment, `X-Email` should be set by an authenticating
    /// proxy such as Oauth2-Proxy. For local development, we allow the header
    /// to be omitted and instead assume this email when no header is present.
    pub unsafe_default_email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    /// The interface address and port to listen on, e.g. `127.0.0.1:5591`.
    pub listen: String,

    /// The url prefix, in case the app is not hosted at the root of a domain.
    ///
    /// E.g. `/hack-o-matic`.
    pub prefix: String,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    /// Path to the database file.
    pub path: String,
}
