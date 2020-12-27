use crate::core::{Error, IWord, RegisterIndex, Result, UWord};
use std::convert::{TryFrom, TryInto};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::io::Read;
use std::slice;

/**
 * The smallest unit of computation that can be fully executed with no
 * extra data required.
 */
pub struct Opcode {
    pub instruction: Instruction,
    pub operands: Vec<Operand>,
}

/**
 * A discrete action that can be performed by the computer.
 * Certain instructions may require operands.
 */
#[repr(u8)]
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Instruction {
    NoOperation = 0x00,
    Move,
    Add,
    Subtract,
    Multiply,
    Divide,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    BitwiseNot,
    ShiftLeft,
    ShiftRight,
    Compare,
    Jump,
    JumpEqual,
    JumpNotEqual,
    JumpGreater,
    JumpGreaterEqual,
    JumpLess,
    JumpLessEqual,
    Call,
    Return,
    Push,
    Pop,
    New,
    GarbageCollector,
    Reference,
    Unreference,
    Halt = 0x3F,
}

/// Useful metadata about an instruction
pub struct InstructionDescriptor {
    /// Operands this instruction expects
    pub operands: &'static [OperandMode],
    /// Mnemonic used to represent this instruction for the user
    pub mnemonic: &'static str,
}

/// Mode of use of an operand
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum OperandMode {
    /// Operand will be read from
    ReadOnly,
    /// Operand will be written to
    ReadWrite,
}

/**
 * An argument used by instructions to identify the location where data will be read
 * or written to.
 */
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Operand {
    /// A hardcoded value that is always the same
    Immediate(IWord),
    /// The value stored in a register
    Register(RegisterIndex),
    /// A location in memory referenced by a register
    Reference {
        /// Register that contains the memory reference
        register: RegisterIndex,
        /// Hardcoded value added to the reference before deferencing it
        offset: IWord,
    },
    /// A stack value
    Stack(UWord),
}

/// Reads a single byte from a reader
fn read_byte(read: &mut impl Read) -> Result<u8> {
    let mut byte = 0u8;
    read.read_exact(slice::from_mut(&mut byte))?;
    Ok(byte)
}

impl Opcode {
    pub fn decode(read: &mut impl Read) -> Result<Opcode> {
        let first_byte = read_byte(read)?;

        let operand_count = ((first_byte & !Instruction::MASK) >> Instruction::SHIFT) as usize;
        let instruction_id = first_byte & Instruction::MASK;

        let instruction = Instruction::decode(instruction_id)?;
        let mut operands = Vec::with_capacity(operand_count);
        for _ in 0..operand_count {
            operands.push(Operand::decode(read)?);
        }

        let descriptor = instruction.descriptor();
        if descriptor.operands.len() != operands.len() {
            return Err(Error::new(&format!(
                "Instruction {} expects {} operands, but {} were provided",
                instruction,
                descriptor.operands.len(),
                operands.len()
            )));
        }

        for i in 0..descriptor.operands.len() {
            let expected = descriptor.operands[i];
            let actual = operands[i];

            if !actual.mode().can_be_used_as(&expected) {
                return Err(Error::new(&format!(
                    "Operand {} cannot be used as {}",
                    actual, expected
                )));
            }
        }

        Ok(Opcode {
            instruction,
            operands,
        })
    }
}

impl Display for Opcode {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        write!(fmt, "{}", self.instruction)?;

        for i in 0..self.operands.len() {
            write!(fmt, " {}", self.operands[i])?;
            if i < self.operands.len() - 1 {
                write!(fmt, ",")?;
            }
        }

        Ok(())
    }
}

impl Instruction {
    const MASK: u8 = 0b0011_111;
    const SHIFT: usize = 6;

    pub fn decode(id: u8) -> Result<Instruction> {
        id.try_into()
    }

    pub fn descriptor(&self) -> InstructionDescriptor {
        match *self {
            Self::NoOperation => InstructionDescriptor {
                mnemonic: "nop",
                operands: &[],
            },
            Self::Halt => InstructionDescriptor {
                mnemonic: "halt",
                operands: &[],
            },
            Self::Add => InstructionDescriptor {
                mnemonic: "add",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::Subtract => InstructionDescriptor {
                mnemonic: "sub",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::Multiply => InstructionDescriptor {
                mnemonic: "mul",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::Divide => InstructionDescriptor {
                mnemonic: "div",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::BitwiseAnd => InstructionDescriptor {
                mnemonic: "and",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::BitwiseOr => InstructionDescriptor {
                mnemonic: "or",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::BitwiseXor => InstructionDescriptor {
                mnemonic: "xor",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::BitwiseNot => InstructionDescriptor {
                mnemonic: "not",
                operands: &[OperandMode::ReadWrite],
            },
            Self::ShiftLeft => InstructionDescriptor {
                mnemonic: "shl",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::ShiftRight => InstructionDescriptor {
                mnemonic: "shr",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::Compare => InstructionDescriptor {
                mnemonic: "cmp",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::Jump => InstructionDescriptor {
                mnemonic: "jmp",
                operands: &[OperandMode::ReadOnly],
            },
            Self::JumpEqual => InstructionDescriptor {
                mnemonic: "jeq",
                operands: &[OperandMode::ReadOnly],
            },
            Self::JumpNotEqual => InstructionDescriptor {
                mnemonic: "jne",
                operands: &[OperandMode::ReadOnly],
            },
            Self::JumpGreater => InstructionDescriptor {
                mnemonic: "jgt",
                operands: &[OperandMode::ReadOnly],
            },
            Self::JumpGreaterEqual => InstructionDescriptor {
                mnemonic: "jge",
                operands: &[OperandMode::ReadOnly],
            },
            Self::JumpLess => InstructionDescriptor {
                mnemonic: "jlt",
                operands: &[OperandMode::ReadOnly],
            },
            Self::JumpLessEqual => InstructionDescriptor {
                mnemonic: "jle",
                operands: &[OperandMode::ReadOnly],
            },
            Self::Call => InstructionDescriptor {
                mnemonic: "call",
                operands: &[OperandMode::ReadOnly],
            },
            Self::Return => InstructionDescriptor {
                mnemonic: "ret",
                operands: &[],
            },
            Self::Move => InstructionDescriptor {
                mnemonic: "mov",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::Push => InstructionDescriptor {
                mnemonic: "push",
                operands: &[OperandMode::ReadOnly],
            },
            Self::Pop => InstructionDescriptor {
                mnemonic: "pop",
                operands: &[OperandMode::ReadWrite],
            },
            Self::New => InstructionDescriptor {
                mnemonic: "new",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
            },
            Self::GarbageCollector => InstructionDescriptor {
                mnemonic: "gc",
                operands: &[],
            },
            Self::Reference => InstructionDescriptor {
                mnemonic: "ref",
                operands: &[OperandMode::ReadWrite],
            },
            Self::Unreference => InstructionDescriptor {
                mnemonic: "unref",
                operands: &[OperandMode::ReadWrite],
            },
        }
    }

    pub fn from_mnemonic(mnemonic: &str) -> Result<Instruction> {
        // TODO: There's got to be a better way to do this
        match mnemonic {
            x if x == Self::NoOperation.descriptor().mnemonic => Ok(Self::NoOperation),
            x if x == Self::Move.descriptor().mnemonic => Ok(Self::Move),
            x if x == Self::Add.descriptor().mnemonic => Ok(Self::Add),
            x if x == Self::Subtract.descriptor().mnemonic => Ok(Self::Subtract),
            x if x == Self::Multiply.descriptor().mnemonic => Ok(Self::Multiply),
            x if x == Self::Divide.descriptor().mnemonic => Ok(Self::Divide),
            x if x == Self::BitwiseAnd.descriptor().mnemonic => Ok(Self::BitwiseAnd),
            x if x == Self::BitwiseOr.descriptor().mnemonic => Ok(Self::BitwiseOr),
            x if x == Self::BitwiseXor.descriptor().mnemonic => Ok(Self::BitwiseXor),
            x if x == Self::BitwiseNot.descriptor().mnemonic => Ok(Self::BitwiseNot),
            x if x == Self::ShiftLeft.descriptor().mnemonic => Ok(Self::ShiftLeft),
            x if x == Self::ShiftRight.descriptor().mnemonic => Ok(Self::ShiftRight),
            x if x == Self::Compare.descriptor().mnemonic => Ok(Self::Compare),
            x if x == Self::Jump.descriptor().mnemonic => Ok(Self::Jump),
            x if x == Self::JumpEqual.descriptor().mnemonic => Ok(Self::JumpEqual),
            x if x == Self::JumpNotEqual.descriptor().mnemonic => Ok(Self::JumpNotEqual),
            x if x == Self::JumpGreater.descriptor().mnemonic => Ok(Self::JumpGreater),
            x if x == Self::JumpGreaterEqual.descriptor().mnemonic => Ok(Self::JumpGreaterEqual),
            x if x == Self::JumpLess.descriptor().mnemonic => Ok(Self::JumpLess),
            x if x == Self::JumpLessEqual.descriptor().mnemonic => Ok(Self::JumpLessEqual),
            x if x == Self::Call.descriptor().mnemonic => Ok(Self::Call),
            x if x == Self::Return.descriptor().mnemonic => Ok(Self::Return),
            x if x == Self::Push.descriptor().mnemonic => Ok(Self::Push),
            x if x == Self::Pop.descriptor().mnemonic => Ok(Self::Pop),
            x if x == Self::New.descriptor().mnemonic => Ok(Self::New),
            x if x == Self::GarbageCollector.descriptor().mnemonic => Ok(Self::GarbageCollector),
            x if x == Self::Reference.descriptor().mnemonic => Ok(Self::Reference),
            x if x == Self::Unreference.descriptor().mnemonic => Ok(Self::Unreference),
            x if x == Self::Halt.descriptor().mnemonic => Ok(Self::Halt),
            x => Err(Error::new(&format!("{:} is not a valid instruction", x))),
        }
    }
}

impl TryFrom<u8> for Instruction {
    type Error = Error;

    fn try_from(byte: u8) -> Result<Self> {
        // TODO: There's got to be a better way to do this
        match byte {
            x if x == Self::NoOperation as u8 => Ok(Self::NoOperation),
            x if x == Self::Move as u8 => Ok(Self::Move),
            x if x == Self::Add as u8 => Ok(Self::Add),
            x if x == Self::Subtract as u8 => Ok(Self::Subtract),
            x if x == Self::Multiply as u8 => Ok(Self::Multiply),
            x if x == Self::Divide as u8 => Ok(Self::Divide),
            x if x == Self::BitwiseAnd as u8 => Ok(Self::BitwiseAnd),
            x if x == Self::BitwiseOr as u8 => Ok(Self::BitwiseOr),
            x if x == Self::BitwiseXor as u8 => Ok(Self::BitwiseXor),
            x if x == Self::BitwiseNot as u8 => Ok(Self::BitwiseNot),
            x if x == Self::ShiftLeft as u8 => Ok(Self::ShiftLeft),
            x if x == Self::ShiftRight as u8 => Ok(Self::ShiftRight),
            x if x == Self::Compare as u8 => Ok(Self::Compare),
            x if x == Self::Jump as u8 => Ok(Self::Jump),
            x if x == Self::JumpEqual as u8 => Ok(Self::JumpEqual),
            x if x == Self::JumpNotEqual as u8 => Ok(Self::JumpNotEqual),
            x if x == Self::JumpGreater as u8 => Ok(Self::JumpGreater),
            x if x == Self::JumpGreaterEqual as u8 => Ok(Self::JumpGreaterEqual),
            x if x == Self::JumpLess as u8 => Ok(Self::JumpLess),
            x if x == Self::JumpLessEqual as u8 => Ok(Self::JumpLessEqual),
            x if x == Self::Call as u8 => Ok(Self::Call),
            x if x == Self::Return as u8 => Ok(Self::Return),
            x if x == Self::Push as u8 => Ok(Self::Push),
            x if x == Self::Pop as u8 => Ok(Self::Pop),
            x if x == Self::New as u8 => Ok(Self::New),
            x if x == Self::GarbageCollector as u8 => Ok(Self::GarbageCollector),
            x if x == Self::Reference as u8 => Ok(Self::Reference),
            x if x == Self::Unreference as u8 => Ok(Self::Unreference),
            x if x == Self::Halt as u8 => Ok(Self::Halt),
            x => Err(Error::new(&format!("{:2x} is not a valid instruction", x))),
        }
    }
}

impl Display for Instruction {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        write!(fmt, "{}", self.descriptor().mnemonic)
    }
}

impl OperandMode {
    /// Checks if one operand mode can be used where another mode is expected
    pub fn can_be_used_as(&self, other: &Self) -> bool {
        // If we're the same, great
        if *self == *other {
            return true;
        }

        // Read/write can be used as read only
        if *self == Self::ReadWrite && *other == Self::ReadOnly {
            return true;
        }

        // Otherwise, not allowed
        return false;
    }
}

impl Display for OperandMode {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        match self {
            OperandMode::ReadOnly => write!(fmt, "read-only"),
            OperandMode::ReadWrite => write!(fmt, "read/write"),
        }
    }
}

impl Operand {
    const ADDRESSING_MODE_MASK: u8 = 0b1100_0000;
    const ADDRESSING_MODE_SHIFT: usize = 6;

    const REGISTER_NUM_MASK: u8 = 0b0011_0000;
    const REGISTER_NUM_SHIFT: usize = 4;

    const SIGN_MASK: u8 = 0b0000_1000;
    const SIGN_SHIFT: usize = 3;

    const VALUE_SIZE_MASK: u8 = 0b0000_0111;
    const VALUE_SIZE_SHIFT: usize = 0;

    fn decode(read: &mut impl Read) -> Result<Operand> {
        let first_byte = read_byte(read)?;

        let addr_mode = (first_byte & Self::ADDRESSING_MODE_MASK) >> Self::ADDRESSING_MODE_SHIFT;
        let register_num = (first_byte & Self::REGISTER_NUM_MASK) >> Self::REGISTER_NUM_SHIFT;
        let sign = (first_byte & Self::SIGN_MASK) >> Self::SIGN_SHIFT;
        let value_size = ((first_byte & Self::VALUE_SIZE_MASK) >> Self::VALUE_SIZE_SHIFT) as usize;

        let mut value_bytes = Vec::with_capacity(value_size);
        for _ in 0..value_size {
            value_bytes.push(0u8);
        }
        read.read_exact(&mut value_bytes)?;

        let mut value_padded_bytes = [0u8; 8];
        for i in 0..value_size {
            value_padded_bytes[i] = value_bytes[i];
        }

        let uvalue = UWord::from_le_bytes(value_padded_bytes);
        let ivalue = uvalue as IWord * if sign == 0 { 1 } else { -1 };

        match addr_mode {
            0b00 => Ok(Operand::Immediate(ivalue)),
            0b01 => Ok(Operand::Register(register_num)),
            0b10 => Ok(Operand::Reference {
                register: register_num,
                offset: ivalue,
            }),
            0b11 => Ok(Operand::Stack(uvalue)),
            x => Err(Error::new(&format!("Invalid addressing mode {:2b}", x))),
        }
    }

    pub fn mode(&self) -> OperandMode {
        match self {
            Operand::Immediate(_) => OperandMode::ReadOnly,
            _ => OperandMode::ReadWrite,
        }
    }
}

impl Display for Operand {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        match self {
            Operand::Immediate(value) => write!(fmt, "{}", value),
            Operand::Register(i) => write!(fmt, "R{}", i),
            Operand::Reference {
                register,
                offset: 0,
            } => write!(fmt, "[R{}]", register),
            Operand::Reference { register, offset } => write!(fmt, "[R{}{:+}]", register, offset),
            Operand::Stack(0) => write!(fmt, "[SP]"),
            Operand::Stack(offset) => write!(fmt, "[SP{:+}]", offset),
        }
    }
}