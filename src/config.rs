use std::env;
use std::fmt;
use std::net::SocketAddr;

use uuid::Uuid;

const DEFAULT_ORG_ID: &str = "00000000-0000-0000-0000-000000000001";
const DEFAULT_HTTP_ADDR: &str = "0.0.0.0:8080";

#[derive(Debug)]
pub enum ConfigError {
    DatabaseUrlMissing,
    InvalidHttpAddr,
    InvalidOrgId,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DatabaseUrlMissing => write!(f, "DATABASE_URL must be set"),
            Self::InvalidHttpAddr => write!(f, "GRIDFORGE_HTTP_ADDR must be a socket address"),
            Self::InvalidOrgId => write!(f, "GRIDFORGE_ORG_ID must be a UUID"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub http_addr: SocketAddr,
    pub org_id: Uuid,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_raw(
            env::var("DATABASE_URL").ok(),
            env::var("GRIDFORGE_HTTP_ADDR").ok(),
            env::var("GRIDFORGE_ORG_ID").ok(),
        )
    }

    pub fn from_raw(
        database_url: Option<String>,
        http_addr: Option<String>,
        org_id: Option<String>,
    ) -> Result<Self, ConfigError> {
        let database_url = database_url.ok_or(ConfigError::DatabaseUrlMissing)?;
        let http_addr = http_addr
            .unwrap_or_else(|| DEFAULT_HTTP_ADDR.to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidHttpAddr)?;
        let org_id = org_id
            .unwrap_or_else(|| DEFAULT_ORG_ID.to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidOrgId)?;

        Ok(Self {
            database_url,
            http_addr,
            org_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_raw_applies_defaults() {
        let config = Config::from_raw(
            Some("postgres://localhost/gridforge".to_string()),
            None,
            None,
        )
        .unwrap();

        assert_eq!(config.database_url, "postgres://localhost/gridforge");
        assert_eq!(
            config.http_addr,
            DEFAULT_HTTP_ADDR.parse::<SocketAddr>().unwrap()
        );
        assert_eq!(config.org_id, DEFAULT_ORG_ID.parse::<Uuid>().unwrap());
    }

    #[test]
    fn from_raw_rejects_a_missing_database_url() {
        let error = Config::from_raw(None, None, None).unwrap_err();
        assert_eq!(error.to_string(), "DATABASE_URL must be set");
    }

    #[test]
    fn from_raw_rejects_invalid_values() {
        let error = Config::from_raw(
            Some("postgres://localhost/gridforge".to_string()),
            Some("not-a-socket-addr".to_string()),
            None,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "GRIDFORGE_HTTP_ADDR must be a socket address"
        );

        let error = Config::from_raw(
            Some("postgres://localhost/gridforge".to_string()),
            None,
            Some("not-a-uuid".to_string()),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "GRIDFORGE_ORG_ID must be a UUID");
    }
}
