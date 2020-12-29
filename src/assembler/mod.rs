use crate::core::Error as CoreError;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::io::{Error as IoError, Read, Seek, Write};

mod lexer;
mod parser;

#[derive(Debug)]
pub struct Error {
    pub message: String,
    pub range: FileRange,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct FilePosition {
    pub line: u64,
    pub column: u64,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct FileRange {
    pub start: FilePosition,
    pub end: FilePosition,
}

type Result<T> = std::result::Result<T, Error>;
type VoidResult = Result<()>;

impl Error {
    fn from_message(msg: &str) -> Error {
        Error {
            message: msg.to_owned(),
            range: FileRange::invalid(),
        }
    }
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "{} {}", self.range, self.message)
    }
}

impl From<Error> for CoreError {
    fn from(e: Error) -> Self {
        let message = format!("{} {}", e.range, e.message);
        CoreError::new(&message)
    }
}

impl From<IoError> for Error {
    fn from(e: IoError) -> Self {
        Error::from_message(&e.to_string())
    }
}

impl From<CoreError> for Error {
    fn from(e: CoreError) -> Self {
        Error {
            message: e.to_string(),
            range: FileRange::invalid(),
        }
    }
}

impl FilePosition {
    fn start() -> FilePosition {
        FilePosition { column: 1, line: 1 }
    }

    fn next_column(&mut self) {
        self.column += 1;
    }

    fn next_line(&mut self) {
        self.column = 1;
        self.line += 1;
    }
}

impl FileRange {
    fn invalid() -> FileRange {
        FileRange {
            start: FilePosition::start(),
            end: FilePosition::start(),
        }
    }

    fn single(value: &FilePosition) -> FileRange {
        FileRange {
            start: value.clone(),
            end: value.clone(),
        }
    }
}

impl Display for FilePosition {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}:{}", self.line, self.column)
    }
}

impl Display for FileRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}-{}", self.start, self.end)
    }
}

pub fn assemble(source: &mut impl Read, result: &mut (impl Write + Seek)) -> VoidResult {
    let tokens = lexer::lex(source)?;
    for opcode in parser::parse(&tokens)? {
        println!("{}", opcode)
    }

    Ok(())
}
