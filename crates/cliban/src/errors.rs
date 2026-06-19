use cliban_core::Error;

/// Map a core error to the Go binary's exit code: 1 not-found, 2 validation, 3 other.
#[allow(dead_code)]
pub fn exit_code_for(err: &Error) -> i32 {
    match err {
        Error::NotFound | Error::ProjectNotFound => 1,
        Error::Validation(_) => 2,
        _ => 3,
    }
}

/// The human message printed after `error: `. Mirrors the Go strings.
#[allow(dead_code)]
pub fn message_for(err: &Error) -> String {
    match err {
        Error::NotFound | Error::ProjectNotFound => "not found".to_string(),
        Error::Validation(pairs) => {
            let detail = pairs.iter().map(|(_, m)| m.clone()).collect::<Vec<_>>().join("; ");
            format!("validation error: {detail}")
        }
        other => other.to_string(),
    }
}

/// A CLI-level error: either a core error or a CLI-constructed one with an explicit exit code.
#[allow(dead_code)]
pub enum CliError {
    Core(Error),
    Coded(i32, String),
}
impl From<Error> for CliError {
    fn from(e: Error) -> Self { CliError::Core(e) }
}
#[allow(dead_code)]
impl CliError {
    pub fn validation(msg: impl Into<String>) -> Self {
        CliError::Coded(2, format!("validation error: {}", msg.into()))
    }
    pub fn not_found(msg: impl Into<String>) -> Self { CliError::Coded(1, msg.into()) }
    pub fn other(msg: impl Into<String>) -> Self { CliError::Coded(3, msg.into()) }
    pub fn code(&self) -> i32 { match self { CliError::Core(e) => exit_code_for(e), CliError::Coded(c, _) => *c } }
    pub fn message(&self) -> String { match self { CliError::Core(e) => message_for(e), CliError::Coded(_, m) => m.clone() } }
}
#[allow(dead_code)]
pub type CliResult<T> = Result<T, CliError>;
