use std::io::{Write, Seek};
use super::parser::{Token, TokenValue, Operand};
use super::{Error, Result, VoidResult}

pub fn encode(tokens: &[Token], write: &mut (impl Write + Seek)) -> VoidResult {

}
