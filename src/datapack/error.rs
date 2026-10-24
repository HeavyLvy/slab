use std::fmt;

/// A problem with a value that cannot be rendered into a valid 26.3 data pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Error { message: message.into() }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::datapack::Error::new(format!($($arg)*)))
    };
}
pub(crate) use bail;

/// Implemented by every AST node that can be structurally invalid in ways the type
/// system doesn't rule out (out-of-range numbers, mixed coordinate kinds, ...).
pub trait Check {
    fn check(&self) -> Result<()>;
}

impl<T: Check> Check for Option<T> {
    fn check(&self) -> Result<()> {
        match self {
            Some(v) => v.check(),
            None => Ok(()),
        }
    }
}

impl<T: Check> Check for Vec<T> {
    fn check(&self) -> Result<()> {
        self.iter().try_for_each(Check::check)
    }
}

impl<T: Check + ?Sized> Check for Box<T> {
    fn check(&self) -> Result<()> {
        (**self).check()
    }
}
