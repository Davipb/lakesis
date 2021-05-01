use crate::core::{Error, IWord, RegisterIndex, Result, UWord};
use std::collections::HashMap;
use std::fmt::{LowerHex, UpperHex, Display, Formatter, Result as FmtResult};
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
#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub enum Instruction {
    NoOperation = 0x00,
    Move = 0x01,
    Add = 0x02,
    Subtract = 0x03,
    Multiply = 0x04,
    Divide = 0x05,
    BitwiseAnd = 0x06,
    BitwiseOr = 0x07,
    BitwiseXor = 0x08,
    BitwiseNot = 0x09,
    ShiftLeft = 0x0A,
    ShiftRight = 0x0B,
    Compare = 0x0C,
    Jump = 0x0D,
    JumpEqual = 0x0E,
    JumpNotEqual = 0x0F,
    JumpGreater = 0x10,
    JumpGreaterEqual = 0x11,
    JumpLess = 0x12,
    JumpLessEqual = 0x13,
    Call = 0x14,
    Return = 0x15,
    Push = 0x16,
    Pop = 0x17,
    New = 0x18,
    GarbageCollector = 0x19,
    Reference = 0x1A,
    Unreference = 0x1B,
    CallNative = 0x1C,
    DebugMemory = 0x3C,
    DebugDump = 0x3D,
    DebugCpu = 0x3E,
    Halt = 0x3F,
}

/// Useful metadata about an instruction
#[derive(PartialEq, Eq, Copy, Clone)]
pub struct InstructionDescriptor {
    /// Operands this instruction expects
    pub operands: &'static [OperandMode],
    /// Mnemonic used to represent this instruction for the user
    pub mnemonic: &'static str,
    /// If this instruction causes a jump
    pub is_jump: bool,
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
        /// Hardcoded value added to the reference before dereferencing it
        offset: IWord,
    },
    /// A stack value
    Stack(UWord),
}

/// Repository of instruction data and metadata
struct InstructionRepository {
    descriptors: HashMap<Instruction, InstructionDescriptor>,
    by_value: HashMap<u8, Instruction>,
    by_mnemonic: HashMap<&'static str, Instruction>,
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

        let is_jump = self.instruction.descriptor().is_jump;
        for i in 0..self.operands.len() {

            if is_jump {
                write!(fmt, " {:X}", self.operands[i])?;
            } else {
                write!(fmt, " {}", self.operands[i])?;
            }

            if i < self.operands.len() - 1 {
                write!(fmt, ",")?;
            }
        }

        Ok(())
    }
}

impl Instruction {
    pub const MASK: u8 = 0b0011_1111;
    pub const SHIFT: usize = 6;

    pub fn decode(value: u8) -> Result<Instruction> {
        Self::from_value(value).ok_or(Error::new(&format!(
            "There is no instruction with value {:2X}",
            value
        )))
    }

    pub fn from_mnemonic(mnemonic: &str) -> Option<Instruction> {
        InstructionRepository::find_by_mnemonic(mnemonic)
    }

    pub fn from_value(value: u8) -> Option<Instruction> {
        InstructionRepository::find_by_value(value)
    }

    pub fn descriptor(&self) -> InstructionDescriptor {
        InstructionRepository::get_descriptor(self)
    }
}

impl Display for Instruction {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        write!(fmt, "{}", self.descriptor().mnemonic)
    }
}

thread_local!(static INSTRUCTION_REPOSITORY: InstructionRepository = InstructionRepository::new());

impl InstructionRepository {

    fn get_descriptor(instr: &Instruction) -> InstructionDescriptor {
        INSTRUCTION_REPOSITORY.with(|r| 
            // All instructions must have a descriptor
            r.descriptors.get(instr).map(Clone::clone).unwrap()
        )
    }

    fn find_by_mnemonic(mnemonic: &str) -> Option<Instruction> {
        INSTRUCTION_REPOSITORY.with(|r| {
            let mnemonic = mnemonic.to_lowercase();
            r.by_mnemonic.get(&mnemonic[..]).map(Clone::clone)
        })
    }

    fn find_by_value(value: u8) -> Option<Instruction> {
        INSTRUCTION_REPOSITORY.with(|r|
            r.by_value.get(&value).map(Clone::clone)
        )
    }

    fn new() -> InstructionRepository {
        let mut descriptors = HashMap::new();
        descriptors.insert(
            Instruction::NoOperation,
            InstructionDescriptor {
                mnemonic: "nop",
                operands: &[],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Halt,
            InstructionDescriptor {
                mnemonic: "halt",
                operands: &[],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Add,
            InstructionDescriptor {
                mnemonic: "add",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Subtract,
            InstructionDescriptor {
                mnemonic: "sub",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Multiply,
            InstructionDescriptor {
                mnemonic: "mul",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Divide,
            InstructionDescriptor {
                mnemonic: "div",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::BitwiseAnd,
            InstructionDescriptor {
                mnemonic: "and",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::BitwiseOr,
            InstructionDescriptor {
                mnemonic: "or",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::BitwiseXor,
            InstructionDescriptor {
                mnemonic: "xor",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::BitwiseNot,
            InstructionDescriptor {
                mnemonic: "not",
                operands: &[OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::ShiftLeft,
            InstructionDescriptor {
                mnemonic: "shl",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::ShiftRight,
            InstructionDescriptor {
                mnemonic: "shr",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Compare,
            InstructionDescriptor {
                mnemonic: "cmp",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadOnly],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Jump,
            InstructionDescriptor {
                mnemonic: "jmp",
                operands: &[OperandMode::ReadOnly],
                is_jump: true,
            },
        );
        descriptors.insert(
            Instruction::JumpEqual,
            InstructionDescriptor {
                mnemonic: "jeq",
                operands: &[OperandMode::ReadOnly],
                is_jump: true,
            },
        );
        descriptors.insert(
            Instruction::JumpNotEqual,
            InstructionDescriptor {
                mnemonic: "jne",
                operands: &[OperandMode::ReadOnly],
                is_jump: true,
            },
        );
        descriptors.insert(
            Instruction::JumpGreater,
            InstructionDescriptor {
                mnemonic: "jgt",
                operands: &[OperandMode::ReadOnly],
                is_jump: true,
            },
        );
        descriptors.insert(
            Instruction::JumpGreaterEqual,
            InstructionDescriptor {
                mnemonic: "jge",
                operands: &[OperandMode::ReadOnly],
                is_jump: true,
            },
        );
        descriptors.insert(
            Instruction::JumpLess,
            InstructionDescriptor {
                mnemonic: "jlt",
                operands: &[OperandMode::ReadOnly],
                is_jump: true,
            },
        );
        descriptors.insert(
            Instruction::JumpLessEqual,
            InstructionDescriptor {
                mnemonic: "jle",
                operands: &[OperandMode::ReadOnly],
                is_jump: true,
            },
        );
        descriptors.insert(
            Instruction::Call,
            InstructionDescriptor {
                mnemonic: "call",
                operands: &[OperandMode::ReadOnly],
                is_jump: true,
            },
        );
        descriptors.insert(
            Instruction::Return,
            InstructionDescriptor {
                mnemonic: "ret",
                operands: &[],
                is_jump: true,
            },
        );
        descriptors.insert(
            Instruction::Move,
            InstructionDescriptor {
                mnemonic: "mov",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Push,
            InstructionDescriptor {
                mnemonic: "push",
                operands: &[OperandMode::ReadOnly],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Pop,
            InstructionDescriptor {
                mnemonic: "pop",
                operands: &[OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::New,
            InstructionDescriptor {
                mnemonic: "new",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::GarbageCollector,
            InstructionDescriptor {
                mnemonic: "gc",
                operands: &[],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Reference,
            InstructionDescriptor {
                mnemonic: "ref",
                operands: &[OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::Unreference,
            InstructionDescriptor {
                mnemonic: "unref",
                operands: &[OperandMode::ReadWrite],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::CallNative,
            InstructionDescriptor {
                mnemonic: "native",
                operands: &[OperandMode::ReadOnly],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::DebugCpu,
            InstructionDescriptor {
                mnemonic: "debugcpu",
                operands: &[OperandMode::ReadOnly],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::DebugDump,
            InstructionDescriptor {
                mnemonic: "debugdump",
                operands: &[OperandMode::ReadOnly, OperandMode::ReadOnly],
                is_jump: false,
            },
        );
        descriptors.insert(
            Instruction::DebugMemory,
            InstructionDescriptor {
                mnemonic: "debugmem",
                operands: &[],
                is_jump: false,
            },
        );

        let mut by_mnemonic = HashMap::new();
        let mut by_value = HashMap::new();

        for (instr, descr) in descriptors.iter() {
            by_mnemonic.insert(descr.mnemonic, instr.clone());
            by_value.insert(*instr as u8, instr.clone());
        }

        InstructionRepository {
            descriptors,
            by_mnemonic,
            by_value,
        }
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
    pub const ADDRESSING_MODE_MASK: u8 = 0b1100_0000;
    pub const ADDRESSING_MODE_SHIFT: usize = 6;

    pub const REGISTER_NUM_MASK: u8 = 0b0011_0000;
    pub const REGISTER_NUM_SHIFT: usize = 4;

    pub const SIGN_MASK: u8 = 0b0000_1000;
    pub const SIGN_SHIFT: usize = 3;

    pub const VALUE_SIZE_MASK: u8 = 0b0000_0111;
    pub const VALUE_SIZE_SHIFT: usize = 0;

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

impl LowerHex for Operand {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        match self {
            Operand::Immediate(value) => write!(fmt, "{:#x}", value),
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

impl UpperHex for Operand {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        match self {
            Operand::Immediate(value) => write!(fmt, "{:#X}", value),
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
