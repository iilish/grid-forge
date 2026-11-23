pub mod config;
pub mod http;

pub const BUILD_SHA: &str = match option_env!("GRIDFORGE_BUILD_SHA") {
    Some(value) => value,
    None => "dev",
};
