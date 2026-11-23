use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CheckError(pub String);

impl CheckError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

pub enum CheckOutcome {
    Pass,
    Pending(String),
}

pub type CheckFn = fn(&Ctx) -> Result<CheckOutcome, CheckError>;

#[derive(Clone, Copy)]
pub struct Check {
    pub id: &'static str,
    pub name: &'static str,
    pub run: CheckFn,
}

pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn preview(&self) -> String {
        let mut preview: String = self.body.chars().take(200).collect();
        if self.body.chars().count() > 200 {
            preview.push('…');
        }
        preview
    }
}

pub struct Ctx {
    pub env_name: String,
    base_url: String,
    env: HashMap<String, String>,
    agent: ureq::Agent,
}

impl Ctx {
    pub fn load(repo_root: &Path, env_name: &str) -> Result<Self, String> {
        let env_path = repo_root
            .join("prove")
            .join("env")
            .join(format!("{env_name}.env"));
        if env_path.is_file() {
            dotenvy::from_path(&env_path)
                .map_err(|error| format!("failed to load {}: {error}", env_path.display()))?;
        } else if env_name != "local" {
            eprintln!(
                "warning: {} not found; checks needing BASE_URL etc. will fail",
                env_path.display()
            );
        }

        let mut env: HashMap<String, String> = std::env::vars().collect();
        env.insert("GP_ENV".to_string(), env_name.to_string());
        if env.get("BASE_URL").is_none_or(|value| value.is_empty()) {
            env.insert("BASE_URL".to_string(), "http://localhost:8080".to_string());
        }
        let base_url = env["BASE_URL"].clone();

        Ok(Self {
            env_name: env_name.to_string(),
            base_url,
            env,
            agent: build_agent(),
        })
    }

    #[cfg(test)]
    pub fn test() -> Self {
        Self {
            env_name: "local".to_string(),
            base_url: "http://localhost:8080".to_string(),
            env: HashMap::new(),
            agent: build_agent(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.env.get(key).map(String::as_str)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    pub fn get_path(&self, path: &str) -> Result<HttpResponse, CheckError> {
        let url = self.url(path);
        let mut response = self
            .agent
            .get(&url)
            .call()
            .map_err(|error| CheckError::new(format!("GET {url}: {error}")))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| CheckError::new(format!("GET {url}: reading body: {error}")))?;
        Ok(HttpResponse { status, body })
    }

    pub fn post_path(
        &self,
        path: &str,
        headers: &[(&str, &str)],
    ) -> Result<HttpResponse, CheckError> {
        let url = self.url(path);
        let mut request = self.agent.post(&url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let mut response = request
            .send_empty()
            .map_err(|error| CheckError::new(format!("POST {url}: {error}")))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| CheckError::new(format!("POST {url}: reading body: {error}")))?;
        Ok(HttpResponse { status, body })
    }
}

fn build_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .http_status_as_error(false)
        .build();
    ureq::Agent::new_with_config(config)
}
