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
        let database_url = env::var("DATABASE_URL").map_err(|_| ConfigError::DatabaseUrlMissing)?;
        let http_addr = env::var("GRIDFORGE_HTTP_ADDR")
            .unwrap_or_else(|_| DEFAULT_HTTP_ADDR.to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidHttpAddr)?;
        let org_id = env::var("GRIDFORGE_ORG_ID")
            .unwrap_or_else(|_| DEFAULT_ORG_ID.to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidOrgId)?;

        Ok(Self {
            database_url,
            http_addr,
            org_id,
        })
    }
}
