use super::{Error, FilePosition, FileRange, Result, VoidResult};
use crate::core::RegisterIndex;
use crate::opcodes::Instruction;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::io::Read;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum TokenValue {
    LabelDefinition(String),
    LabelReference(String),
    Instruction(Instruction),
    Number(i64),
    Register(RegisterIndex),
    StackPointer,
    StartReference,
    EndReference,
    ArgumentSeparator,
    OffsetPositive,
    OffsetNegative,
    AssemblerInstruction(AssemblerInstruction),
    StringLiteral(String),
}

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum AssemblerInstruction {
    String,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Token {
    pub value: TokenValue,
    pub range: FileRange,
}

#[derive(Debug)]
struct TrackingFileReader {
    buffer: Vec<char>,
    index: usize,
    pos: FilePosition,
}

#[derive(Debug)]
struct Lexer {
    reader: TrackingFileReader,
    tokens: Vec<Token>,
    token_start: FilePosition,
    inside_ref: bool,
}

impl Display for TokenValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:#?}", self)
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{} {}", self.range, self.value)
    }
}

impl TrackingFileReader {
    fn from_reader(reader: &mut impl Read) -> Result<TrackingFileReader> {
        let mut byte_buffer = Vec::new();
        reader.read_to_end(&mut byte_buffer)?;
        let char_buffer = String::from_utf8_lossy(&byte_buffer).chars().collect();

        Ok(Self::from_buffer(char_buffer))
    }

    fn from_buffer(buffer: Vec<char>) -> TrackingFileReader {
        TrackingFileReader {
            buffer,
            index: 0,
            pos: FilePosition::start(),
        }
    }

    fn is_eof(&self) -> bool {
        self.index >= self.buffer.len()
    }

    fn position(&self) -> FilePosition {
        self.pos.clone()
    }

    fn peek(&self) -> char {
        self.peek_around(0)
    }

    fn peek_around(&self, offset: isize) -> char {
        let result = self.peek_around_raw(offset);

        // Normalize line endings to \n
        if result == '\r' {
            '\n'
        } else {
            result
        }
    }

    /// Peek without end-of-line normalization
    fn peek_raw(&self) -> char {
        self.peek_around_raw(0)
    }

    /// Peek around without end-of-line normalization
    fn peek_around_raw(&self, offset: isize) -> char {
        self.buffer
            .get(((self.index as isize) + offset) as usize)
            .map(Clone::clone)
            .unwrap_or('\0')
    }

    /// Consumes the current character. Returns true if there are more characters coming next
    fn consume(&mut self) -> bool {
        if self.is_eof() {
            return false;
        }

        if self.peek() == '\r' || self.peek() == '\n' {
            self.pos.next_line();
        } else {
            self.pos.next_column();
        }

        // Treat \r\n as a single logical character

        if self.peek_raw() == '\r' && self.peek_around_raw(1) == '\n' {
            self.index += 2;
        } else {
            self.index += 1;
        }

        !self.is_eof()
    }

    fn consume_many(&mut self, amount: usize) -> bool {
        for _ in 0..amount {
            if !self.consume() {
                return false;
            }
        }

        true
    }

    fn consume_or_error(&mut self) -> VoidResult {
        self.consume_many_or_error(1)
    }

    fn consume_many_or_error(&mut self, amount: usize) -> VoidResult {
        if self.consume_many(amount) {
            Ok(())
        } else {
            Err(Error {
                message: "Unexpected end of file".to_owned(),
                range: FileRange::single(&self.pos),
            })
        }
    }
}

impl Lexer {
    fn new(reader: TrackingFileReader) -> Lexer {
        Lexer {
            reader,
            tokens: Vec::new(),
            token_start: FilePosition::start(),
            inside_ref: false,
        }
    }

    fn range(&self) -> FileRange {
        FileRange {
            start: self.token_start,
            end: self.reader.position(),
        }
    }

    fn make_token(&mut self, value: TokenValue) {
        self.tokens.push(Token {
            value,
            range: self.range(),
        })
    }

    fn make_error(&self, msg: &str) -> Error {
        Error {
            message: msg.to_owned(),
            range: self.range(),
        }
    }

    fn lex(mut self) -> Result<Vec<Token>> {
        if !self.tokens.is_empty() {
            return Err(Error::from_message(
                "Lexer cannot be reused after lex() is called",
            ));
        }

        while !self.reader.is_eof() {
            self.token_start = self.reader.position();
            self.lex_single()?;
        }

        Ok(self.tokens)
    }

    fn lex_single(&mut self) -> VoidResult {
        if self.reader.peek().is_whitespace() {
            self.lex_whitespace();
            return Ok(());
        }
        if self.reader.peek() == ';' {
            self.lex_comment();
            return Ok(());
        }

        if self.reader.peek() == '[' {
            self.reader.consume();
            self.inside_ref = true;
            self.make_token(TokenValue::StartReference);
            return Ok(());
        }

        if self.reader.peek() == ']' {
            self.reader.consume();
            self.inside_ref = false;
            self.make_token(TokenValue::EndReference);
            return Ok(());
        }

        if self.reader.peek() == ',' {
            self.reader.consume();
            self.make_token(TokenValue::ArgumentSeparator);
            return Ok(());
        }

        if self.reader.peek() == '+' {
            if !self.inside_ref {
                return self.lex_number();
            }

            self.reader.consume();
            self.make_token(TokenValue::OffsetPositive);
            return Ok(());
        }

        if self.reader.peek() == '-' {
            if !self.inside_ref {
                return self.lex_number();
            }
            self.reader.consume();
            self.make_token(TokenValue::OffsetNegative);
            return Ok(());
        }

        if self.reader.peek() == '.' {
            self.reader.consume();
            return self.lex_assembler_instruction();
        }

        if self.reader.peek() == '"' {
            self.reader.consume();
            return self.lex_string();
        }

        if self.reader.peek().is_digit(10) {
            return self.lex_number();
        }

        if Self::is_valid_identifier_start(self.reader.peek()) {
            return self.lex_identifier();
        }

        Err(self.make_error("Syntax error"))
    }

    fn lex_whitespace(&mut self) {
        while self.reader.peek().is_whitespace() && self.reader.consume() {}
    }

    fn lex_comment(&mut self) {
        while self.reader.peek() != '\n' && self.reader.consume() {}
    }

    fn lex_number(&mut self) -> VoidResult {
        let is_positive = if self.reader.peek() == '+' {
            self.reader.consume_or_error()?;
            true
        } else if self.reader.peek() == '-' {
            self.reader.consume_or_error()?;
            false
        } else {
            true
        };

        let radix = if self.reader.peek() == '0' && self.reader.peek_around(1) == 'x' {
            self.reader.consume_many_or_error(2)?;
            16
        } else {
            10
        };

        let mut digits = String::new();
        while self.reader.peek().is_alphanumeric() {
            digits.push(self.reader.peek());
            if !self.reader.consume() {
                break;
            }
        }

        if digits.is_empty() {
            return Err(self.make_error("Numbers need at least one digit"));
        }

        let raw_num = match i64::from_str_radix(&digits, radix) {
            Err(e) => return Err(self.make_error(&e.to_string())),
            Ok(x) => x,
        };

        let num = if is_positive { raw_num } else { -raw_num };
        self.make_token(TokenValue::Number(num));

        Ok(())
    }

    fn lex_string(&mut self) -> VoidResult {
        let mut string = String::new();

        while self.reader.peek() != '"' {
            if self.reader.peek() == '\\' {
                self.reader.consume_or_error()?;

                match self.reader.peek() {
                    'n' => string.push('\n'),
                    '"' => string.push('\"'),
                    '\\' => string.push('\\'),
                    x => return Err(self.make_error(&format!("Unknown escape sequence \\{}", x))),
                }
            } else {
                string.push(self.reader.peek());
            }

            self.reader.consume_or_error()?;
        }

        self.reader.consume();
        self.make_token(TokenValue::StringLiteral(string));

        Ok(())
    }

    fn lex_identifier(&mut self) -> VoidResult {
        let mut name = String::new();

        while Self::is_valid_identifier_middle(self.reader.peek()) {
            name.push(self.reader.peek());
            if !self.reader.consume() {
                break;
            }
        }

        if name.ends_with(':') {
            let label_name = name.trim_end_matches(':').to_owned();
            self.make_token(TokenValue::LabelDefinition(label_name));
            return Ok(());
        }

        if self.lex_register(&name)? || self.lex_instruction(&name) {
            return Ok(());
        }

        if name.eq_ignore_ascii_case("SP") {
            self.make_token(TokenValue::StackPointer);
            return Ok(());
        }

        self.make_token(TokenValue::LabelReference(name));
        Ok(())
    }

    fn lex_assembler_instruction(&mut self) -> VoidResult {
        let mut name = String::new();

        while self.reader.peek().is_ascii_alphabetic() {
            name.push(self.reader.peek());
            if !self.reader.consume() {
                break;
            }
        }

        if name.eq_ignore_ascii_case("string") {
            self.make_token(TokenValue::AssemblerInstruction(
                AssemblerInstruction::String,
            ));
            return Ok(());
        }

        Err(self.make_error(&format!("Unknown assembler instruction '{}'", name)))
    }

    fn lex_register(&mut self, identifier: &str) -> Result<bool> {
        if !identifier.starts_with('R') && !identifier.starts_with('r') {
            return Ok(false);
        }

        let reg_num_str = identifier.trim_start_matches('R').trim_start_matches('r');
        if !reg_num_str.chars().count() == 1 {
            return Ok(false);
        }

        let reg_num: RegisterIndex = match reg_num_str.parse() {
            Err(_) => return Ok(false),
            Ok(x) => x,
        };

        // TODO: Move this to a constant somewhere
        if reg_num > 3 {
            return Err(
                self.make_error("Invalid register index. Must be a number from 0 to 3 inclusive.")
            );
        }

        self.make_token(TokenValue::Register(reg_num));
        Ok(true)
    }

    fn lex_instruction(&mut self, identifier: &str) -> bool {
        let instruction = match Instruction::from_mnemonic(identifier) {
            None => return false,
            Some(x) => x,
        };

        self.make_token(TokenValue::Instruction(instruction));
        true
    }

    fn is_valid_identifier_start(c: char) -> bool {
        c == '_' || c.is_alphabetic()
    }

    fn is_valid_identifier_middle(c: char) -> bool {
        Self::is_valid_identifier_start(c) || c.is_numeric() || c == ':'
    }
}

pub fn lex(read: &mut impl Read) -> Result<Vec<Token>> {
    let reader = TrackingFileReader::from_reader(read)?;
    Lexer::new(reader).lex()
}
