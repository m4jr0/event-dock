use super::AppResult;
use crate::cli::QueryArgs;
use eventdock_engine::{QueryRequest, query_remote};

pub fn run(args: QueryArgs) -> AppResult<()> {
    let response = query_remote(
        &args.address,
        QueryRequest {
            limit: args.limit,
            before_id: args.before_id,
            level: args.level.map(Into::into),
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
