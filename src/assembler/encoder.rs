use super::parser::{Operand, Token, TokenValue};
use super::{Error, FilePosition, FileRange, Result, VoidResult};
use crate::core::{IWord, UWord};
use crate::opcodes::{Instruction, Operand as CoreOperand};
use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Write};
use std::slice;

struct Encoder<'a, T>
where
    T: Write + Seek,
{
    tokens: &'a [Token],
    output: &'a mut T,
    index: usize,
    label_values: HashMap<String, u64>,
    fixups: HashMap<u64, String>,
}

struct OperandData<'a> {
    addressing_mode: u8,
    register_number: u8,
    value_is_positive: bool,
    value_absolute: UWord,
    label: Option<&'a str>,
}

impl<T> Encoder<'_, T>
where
    T: Write + Seek,
{
    fn new<'a>(tokens: &'a [Token], output: &'a mut T) -> Encoder<'a, T> {
        Encoder {
            tokens,
            output,
            index: 0,
            label_values: HashMap::new(),
            fixups: HashMap::new(),
        }
    }

    fn offset(&mut self) -> Result<u64> {
        Ok(self.output.seek(SeekFrom::Current(0))?)
    }

    fn is_eof(&self) -> bool {
        self.index >= self.tokens.len()
    }

    fn peek(&self) -> &TokenValue {
        &self.peek_full().value
    }

    fn peek_full(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn range(&self) -> FileRange {
        if self.is_eof() {
            self.tokens[self.tokens.len() - 1].range
        } else {
            self.peek_full().range
        }
    }

    fn make_error(&self, msg: &str) -> Error {
        Error {
            message: msg.to_owned(),
            range: self.range(),
        }
    }

    fn consume(&mut self) -> bool {
        if self.is_eof() {
            return false;
        }

        self.index += 1;
        !self.is_eof()
    }

    fn write(&mut self, bytes: &[u8]) -> VoidResult {
        self.output.write_all(bytes)?;
        Ok(())
    }

    fn write_byte(&mut self, byte: u8) -> VoidResult {
        self.write(slice::from_ref(&byte))
    }

    fn encode(mut self) -> VoidResult {
        while !self.is_eof() {
            self.encode_single()?;
        }

        self.fixup()?;
        Ok(())
    }

    fn encode_single(&mut self) -> VoidResult {
        match self.peek().clone() {
            TokenValue::Label(s) => self.remember_label(&s)?,
            TokenValue::Define { label, value } => {
                self.set_label_value_without_override(&label, value as u64)?
            }
            TokenValue::String {
                length_label,
                value,
            } => self.encode_string(length_label.as_ref(), &value)?,
            TokenValue::Align(n) => self.align_output(n)?,
            TokenValue::Opcode {
                instruction,
                operands,
            } => self.encode_opcode(instruction, &operands)?,
        }

        self.consume();
        Ok(())
    }

    fn align_output(&mut self, alignment: UWord) -> VoidResult {
        if alignment <= 1 {
            return Err(self.make_error("Alignment must be bigger than 1"));
        }

        while self.offset()? % alignment != 0 {
            self.write_byte(0)?;
        }

        Ok(())
    }

    fn remember_label(&mut self, name: &str) -> VoidResult {
        let offset = self.offset()?;
        self.set_label_value_without_override(name, offset)
    }

    fn set_label_value_without_override(&mut self, name: &str, value: u64) -> VoidResult {
        match self.label_values.insert(name.to_owned(), value) {
            None => Ok(()),
            Some(_) => Err(self.make_error(&format!("Redefinition of label {}", name))),
        }
    }

    fn encode_string(&mut self, length_label: Option<&String>, value: &str) -> VoidResult {
        let bytes = value.as_bytes();
        if let Some(label) = length_label {
            self.set_label_value_without_override(label, bytes.len() as u64)?;
        }

        self.write(bytes)
    }

    fn encode_opcode(&mut self, instr: Instruction, operands: &[Operand]) -> VoidResult {
        let mut value = instr as u8 & Instruction::MASK;
        value |= ((operands.len() as u8) << Instruction::SHIFT) & !Instruction::MASK;

        self.write_byte(value)?;
        for operand in operands {
            self.encode_operand(operand)?;
        }

        Ok(())
    }

    fn encode_operand(&mut self, operand: &Operand) -> VoidResult {
        let data = Self::get_operand_data(operand);
        let mut first_byte = 0;

        first_byte |= (data.addressing_mode << CoreOperand::ADDRESSING_MODE_SHIFT)
            & CoreOperand::ADDRESSING_MODE_MASK;

        first_byte |= (data.register_number << CoreOperand::REGISTER_NUM_SHIFT)
            & CoreOperand::REGISTER_NUM_MASK;

        if !data.value_is_positive {
            first_byte |= CoreOperand::SIGN_MASK;
        }

        let mut value_bytes: Vec<u8> = data.value_absolute.to_le_bytes().iter().cloned().collect();
        if data.label.is_some() {
            value_bytes.pop();
        } else {
            while value_bytes.ends_with(&[0]) {
                value_bytes.pop();
            }
        }

        if value_bytes.len() > 7 {
            return Err(self.make_error("Operand value cannot be longer than 7 bytes"));
        }

        first_byte |= ((value_bytes.len() as u8) << CoreOperand::VALUE_SIZE_SHIFT)
            & CoreOperand::VALUE_SIZE_MASK;

        self.write_byte(first_byte)?;

        let offset = self.offset()?;
        if let Some(label) = data.label {
            self.fixups.insert(offset, label.to_owned());
        }

        self.write(&value_bytes)?;
        Ok(())
    }

    fn get_operand_data(operand: &Operand) -> OperandData {
        match operand {
            Operand::Label(l) => OperandData {
                addressing_mode: 0,
                register_number: 0,
                value_is_positive: true,
                value_absolute: 0,
                label: Some(l),
            },
            Operand::Immediate(x) => OperandData {
                addressing_mode: 0,
                register_number: 0,
                value_is_positive: *x >= 0,
                value_absolute: x.abs() as UWord,
                label: None,
            },
            Operand::Register(r) => OperandData {
                addressing_mode: 1,
                register_number: *r,
                value_is_positive: true,
                value_absolute: 0,
                label: None,
            },
            Operand::Reference { register, offset } => OperandData {
                addressing_mode: 2,
                register_number: *register,
                value_is_positive: *offset >= 0,
                value_absolute: offset.abs() as UWord,
                label: None,
            },
            Operand::Stack(o) => OperandData {
                addressing_mode: 3,
                register_number: 0,
                value_is_positive: true,
                value_absolute: *o,
                label: None,
            },
        }
    }

    fn fixup(&mut self) -> VoidResult {
        for (offset, label) in &self.fixups {
            let label_value = match self.label_values.get(label) {
                Some(x) => *x,
                None => return Err(Error::from_message(&format!("Label {} not found", label))),
            };

            self.output.seek(SeekFrom::Start(*offset))?;
            let bytes = label_value.to_le_bytes();
            self.output.write_all(&bytes[0..7])?;
        }

        Ok(())
    }
}

pub fn encode(tokens: &[Token], output: &mut (impl Write + Seek)) -> VoidResult {
    Encoder::new(tokens, output).encode()
}
