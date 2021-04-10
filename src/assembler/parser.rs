use super::lexer::{
    Directive as LexerDirective, Token as LexerToken, TokenValue as LexerTokenValue,
};
use super::{Error, FilePosition, FileRange, Result, VoidResult};
use crate::core::{IWord, RegisterIndex, UWord};
use crate::opcodes::{Instruction, OperandMode};
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(PartialEq, Eq, Clone)]
pub struct Token {
    pub value: TokenValue,
    pub range: FileRange,
}

#[derive(PartialEq, Eq, Clone)]
pub enum TokenValue {
    Label(String),
    String {
        length_label: Option<String>,
        value: String,
    },
    Align(UWord),
    Define {
        label: String,
        value: IWord,
    },
    Opcode {
        instruction: Instruction,
        operands: Vec<Operand>,
    },
}

#[derive(PartialEq, Eq, Clone)]
pub enum Operand {
    Label(String),
    Immediate(IWord),
    Register(RegisterIndex),
    Stack(UWord),
    Reference {
        register: RegisterIndex,
        offset: IWord,
    },
}

pub struct Parser<'a> {
    inputs: &'a [LexerToken],
    input_index: usize,
    token_start: FilePosition,
    outputs: Vec<Token>,
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{} {}", self.range, self.value)
    }
}

impl Display for TokenValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Label(label) => write!(f, "{}:", label),
            Self::Define { label, value } => write!(f, ".define {} {}", label, value),
            Self::String {
                length_label,
                value,
            } => {
                write!(f, ".string ")?;

                if let Some(label) = length_label {
                    write!(f, "{} ", label)?;
                }

                write!(f, "\"{}\"", value.escape_default())
            }

            Self::Align(alignment) => write!(f, ".align {}", alignment),
            Self::Opcode {
                instruction,
                operands,
            } => {
                write!(f, "    {}", instruction)?;
                for i in 0..operands.len() {
                    if i > 0 {
                        write!(f, ",")?;
                    }

                    write!(f, " {}", &operands[i])?;
                }

                Ok(())
            }
        }
    }
}

impl Operand {
    fn mode(&self) -> OperandMode {
        match self {
            Self::Immediate(_) | Self::Label(_) => OperandMode::ReadOnly,
            _ => OperandMode::ReadWrite,
        }
    }
}

impl Display for Operand {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Operand::Label(x) => write!(f, "{}", x),
            Operand::Immediate(value) => write!(f, "{}", value),
            Operand::Register(i) => write!(f, "R{}", i),
            Operand::Reference {
                register,
                offset: 0,
            } => write!(f, "[R{}]", register),
            Operand::Reference { register, offset } => {
                write!(f, "[R{}{:+}]", register, offset)
            }
            Operand::Stack(0) => write!(f, "[SP]"),
            Operand::Stack(offset) => write!(f, "[SP{:+}]", offset),
        }
    }
}

impl Parser<'_> {
    fn new(inputs: &[LexerToken]) -> Parser {
        Parser {
            inputs,
            input_index: 0,
            token_start: FilePosition::start(),
            outputs: Vec::new(),
        }
    }

    fn is_eof(&self) -> bool {
        self.input_index >= self.inputs.len()
    }
    fn peek(&self) -> &LexerTokenValue {
        &self.peek_full().value
    }

    fn peek_full(&self) -> &LexerToken {
        &self.inputs[self.input_index]
    }

    fn consume(&mut self) -> bool {
        if self.is_eof() {
            return false;
        }

        self.input_index += 1;
        !self.is_eof()
    }

    fn consume_or_error(&mut self) -> VoidResult {
        if self.consume() {
            Ok(())
        } else {
            Err(self.make_error("Unexpected end of file"))
        }
    }

    fn range(&self) -> FileRange {
        let end = if self.is_eof() {
            self.inputs[self.inputs.len() - 1].range.end
        } else {
            self.peek_full().range.end
        };

        FileRange {
            start: self.token_start,
            end,
        }
    }
    fn make_token(&mut self, value: TokenValue) {
        self.outputs.push(Token {
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

    fn parse(mut self) -> Result<Vec<Token>> {
        if !self.outputs.is_empty() {
            return Err(Error::from_message(
                "Parser cannot be reused after parse() is called",
            ));
        }

        while !self.is_eof() {
            self.token_start = self.peek_full().range.start;
            self.parse_single()?;
        }

        Ok(self.outputs)
    }

    fn parse_single(&mut self) -> VoidResult {
        if let LexerTokenValue::LabelDefinition(label) = self.peek() {
            let label = label.to_owned();
            self.make_token(TokenValue::Label(label));
            self.consume();
            return Ok(());
        }

        if let LexerTokenValue::Directive(instruction) = self.peek() {
            return self.parse_directive();
        }

        if let LexerTokenValue::Instruction(_) = self.peek() {
            return self.parse_opcode();
        }

        Err(self.make_error("Expected label or instruction"))
    }

    fn parse_directive(&mut self) -> VoidResult {
        let directive = match self.peek() {
            LexerTokenValue::Directive(x) => *x,
            _ => return Err(self.make_error("Expected directive")),
        };

        self.consume_or_error()?;

        match directive {
            LexerDirective::String => self.parse_directive_string(),
            LexerDirective::Align => self.parse_directive_align(),
            LexerDirective::Define => self.parse_directive_define(),
        }
    }

    fn parse_directive_string(&mut self) -> VoidResult {
        let length_label = match self.peek() {
            LexerTokenValue::LabelReference(s) => {
                let owned = s.to_owned();
                self.consume_or_error()?;
                Some(owned)
            }
            _ => None,
        };

        let value = match self.peek() {
            LexerTokenValue::StringLiteral(str) => str.to_owned(),
            _ => return Err(self.make_error("Expected string literal")),
        };

        self.consume();
        self.make_token(TokenValue::String {
            length_label,
            value,
        });

        Ok(())
    }

    fn parse_directive_align(&mut self) -> VoidResult {
        let alignment = match self.peek() {
            LexerTokenValue::Number(n) => *n,
            _ => return Err(self.make_error("Expected a number")),
        };

        if alignment <= 1 {
            return Err(self.make_error("Alignment must be bigger than 1"));
        }

        self.consume();
        self.make_token(TokenValue::Align(alignment as UWord));

        Ok(())
    }

    fn parse_directive_define(&mut self) -> VoidResult {
        let label = match self.peek() {
            LexerTokenValue::LabelReference(l) => l.to_owned(),
            _ => return Err(self.make_error("Expected a label")),
        };

        self.consume_or_error()?;

        let value = match self.peek() {
            LexerTokenValue::Number(n) => *n,
            _ => return Err(self.make_error("Expected a number")),
        };

        self.consume();
        self.make_token(TokenValue::Define { label, value });

        Ok(())
    }

    fn parse_opcode(&mut self) -> VoidResult {
        let instruction = match self.peek() {
            LexerTokenValue::Instruction(x) => *x,
            _ => return Err(self.make_error("Expected instruction")),
        };

        self.consume();
        let mut operands = Vec::new();

        loop {
            match self.parse_operand()? {
                Some(x) => operands.push(x),
                None if operands.is_empty() => break,
                None => return Err(self.make_error("Expected operand")),
            }

            if self.is_eof() {
                break;
            }

            match self.peek() {
                LexerTokenValue::ArgumentSeparator => self.consume_or_error()?,
                _ => break,
            }
        }

        let descriptor = instruction.descriptor();
        if descriptor.operands.len() != operands.len() {
            return Err(self.make_error(&format!(
                "{} expects {} operand(s), but {} were provided",
                descriptor.mnemonic,
                descriptor.operands.len(),
                operands.len()
            )));
        }

        for i in 0..descriptor.operands.len() {
            let expected = descriptor.operands[i];
            let actual = operands[i].mode();

            if !actual.can_be_used_as(&expected) {
                return Err(self.make_error(&format!(
                    "{}'s operand {} is {}, but {} was provided",
                    descriptor.mnemonic,
                    i + 1,
                    expected,
                    operands[i]
                )));
            }
        }

        self.make_token(TokenValue::Opcode {
            instruction,
            operands,
        });

        Ok(())
    }

    fn parse_operand(&mut self) -> Result<Option<Operand>> {
        if self.is_eof() {
            return Ok(None);
        }

        let mut consume = true;

        let found_operand = match self.peek() {
            LexerTokenValue::LabelReference(label) => Some(Operand::Label(label.clone())),
            LexerTokenValue::Number(n) => Some(Operand::Immediate(*n)),
            LexerTokenValue::Register(i) => Some(Operand::Register(*i)),
            LexerTokenValue::CharacterLiteral(c) => Some(Operand::Immediate(*c as IWord)),
            LexerTokenValue::StartReference => {
                consume = false;
                Some(self.parse_reference_or_stack()?)
            }
            _ => {
                consume = false;
                None
            }
        };

        if consume {
            self.consume();
        }

        Ok(found_operand)
    }

    fn parse_reference_or_stack(&mut self) -> Result<Operand> {
        match self.peek() {
            LexerTokenValue::StartReference => {}
            _ => return Err(self.make_error("Expected reference start")),
        }

        self.consume_or_error()?;

        let register = match self.peek() {
            LexerTokenValue::StackPointer => None,
            LexerTokenValue::Register(r) => Some(*r),
            _ => return Err(self.make_error("Expected stack pointer or register")),
        };

        self.consume_or_error()?;

        let offset = self.parse_reference_or_stack_offset()?;
        if offset < 0 && register.is_none() {
            return Err(self.make_error("Stack pointer offsets cannot be negative"));
        }

        match self.peek() {
            LexerTokenValue::EndReference => {}
            _ => return Err(self.make_error("Expected end of reference")),
        }

        self.consume();

        match register {
            None => Ok(Operand::Stack(offset as UWord)),
            Some(r) => Ok(Operand::Reference {
                register: r,
                offset,
            }),
        }
    }

    fn parse_reference_or_stack_offset(&mut self) -> Result<IWord> {
        let is_negative = match self.peek() {
            LexerTokenValue::OffsetPositive => false,
            LexerTokenValue::OffsetNegative => true,
            _ => return Ok(0),
        };

        self.consume_or_error()?;

        let absolute_value = match self.peek() {
            LexerTokenValue::Number(n) => *n,
            _ => return Err(self.make_error("Expected number")),
        };

        self.consume_or_error()?;

        if is_negative {
            Ok(-absolute_value)
        } else {
            Ok(absolute_value)
        }
    }
}

pub fn parse(tokens: &[LexerToken]) -> Result<Vec<Token>> {
    Parser::new(tokens).parse()
}
