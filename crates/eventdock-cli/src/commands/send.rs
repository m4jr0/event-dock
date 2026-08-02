use super::AppResult;
use crate::cli::SendArgs;
use eventdock::{IncomingEvent, Logger, TcpSink};
use std::io;

pub fn run(args: SendArgs) -> AppResult<()> {
    let mut event = IncomingEvent::new(
        args.source.clone(),
        args.level.into(),
        args.message,
    );

    for field in args.fields {
        let (name, value) = parse_field_argument(&field)?;
        event.insert_field(name, value);
    }

    let sink = TcpSink::connect(&*args.address)?;
    let mut logger = Logger::new(args.source, sink);
    logger.log_event(event)?;
    logger.flush()?;
    println!("Event sent to {}", args.address);
    Ok(())
}

fn parse_field_argument(field: &str) -> io::Result<(String, String)> {
    let (name, value) = field.split_once('=').ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid field '{field}'; expected KEY=VALUE"),
        )
    })?;

    if name.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "field names cannot be empty",
        ));
    }

    Ok((name.to_owned(), value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_parser_preserves_equals_in_value() {
        assert_eq!(
            parse_field_argument("a=b=c").unwrap(),
            ("a".into(), "b=c".into())
        );
    }

    #[test]
    fn field_parser_accepts_empty_value() {
        assert_eq!(
            parse_field_argument("a=").unwrap(),
            ("a".into(), "".into())
        );
    }

    #[test]
    fn field_parser_rejects_missing_separator_or_name() {
        assert!(parse_field_argument("abc").is_err());
        assert!(parse_field_argument("=value").is_err());
    }
}
