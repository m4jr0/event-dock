pub mod query;
pub mod send;
pub mod serve;

pub type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
