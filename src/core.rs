use std::fmt::{self, Display, Formatter};
use std::result;

pub type UWord = u64;
pub type IWord = i64;
pub type RegisterIndex = u8;

pub const REGISTER_NUM: usize = 4;

#[derive(Debug)]
pub struct Error {
    message: Option<String>,
}

pub type Result<T> = result::Result<T, Error>;
pub type VoidResult = Result<()>;

impl Error {
    pub fn new(msg: &str) -> Error {
        Error {
            message: Some(msg.to_owned()),
        }
    }
}

impl Display for Error {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        write!(
            fmt,
            "{}",
            match &self.message {
                None => "Unknown error",
                Some(x) => x,
            }
        )?;
        Ok(())
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new(&e.to_string())
    }
}

impl From<Error> for std::io::Error {
    fn from(e: Error) -> Self {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    }
}
