use crate::core::{Error, IWord, Result, UWord, VoidResult, REGISTER_NUM, WORD_BYTE_SIZE};
use crate::opcodes::{Instruction, Opcode, Operand};
use memory::Memory;
use rand::prelude::*;
use std::fmt::{Display, Formatter, UpperHex};
use std::io::{self, Read};
use std::num::Wrapping;
use std::ops::{Add, BitAnd, BitOr, BitXor, Div, Mul, Not, Shl, Shr, Sub};
use std::thread;
use std::time::Duration;

mod memory;

const STACK_SIZE: UWord = WORD_BYTE_SIZE * 0xFF;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct DataValue<T> {
    value: T,
    is_reference: bool,
}

pub type DataWord = DataValue<UWord>;

#[derive(Clone, Debug, Default)]
struct CpuState {
    registers: [DataWord; REGISTER_NUM],
    stack_pointer: Wrapping<UWord>,
    instruction_pointer: Wrapping<UWord>,
    carry_flag: bool,
    zero_flag: bool,
}

#[derive(Debug)]
struct Interpreter {
    cpu_state: CpuState,
    memory: Memory,
}

struct InterpreterInstructionPointerReader<'a> {
    memory: &'a Memory,
    cpu_state: &'a mut CpuState,
}

impl<T> DataValue<T> {
    pub fn expect_reference(self) -> Result<T> {
        if !self.is_reference {
            Err(Error::new("Expected a reference, but found data"))
        } else {
            Ok(self.value)
        }
    }

    pub fn expect_data(self) -> Result<T> {
        if self.is_reference {
            Err(Error::new("Expected data, but found a reference"))
        } else {
            Ok(self.value)
        }
    }

    pub fn map<TOutput>(self, map: impl FnOnce(T) -> TOutput) -> DataValue<TOutput> {
        DataValue {
            value: map(self.value),
            is_reference: self.is_reference,
        }
    }

    pub fn combine<TOther, TOutput>(
        self,
        other: DataValue<TOther>,
        combiner: impl FnOnce(T, TOther) -> TOutput,
    ) -> DataValue<TOutput> {
        DataValue {
            value: combiner(self.value, other.value),
            is_reference: self.is_reference || other.is_reference,
        }
    }

    fn overflowing_operation(
        self,
        other: DataValue<T>,
        op: impl FnOnce(T, T) -> (T, bool),
    ) -> (DataValue<T>, bool) {
        let mut carry = false;
        let result = self.combine(other, |a, b| {
            let (value, carry_inner) = op(a, b);
            carry = carry_inner;
            value
        });
        (result, carry)
    }
}

impl<T, TRhs> Add<DataValue<TRhs>> for DataValue<T>
where
    T: Add<TRhs>,
{
    type Output = DataValue<T::Output>;

    fn add(self, other: DataValue<TRhs>) -> Self::Output {
        self.combine(other, T::add)
    }
}

impl<T, TRhs> Sub<DataValue<TRhs>> for DataValue<T>
where
    T: Sub<TRhs>,
{
    type Output = DataValue<T::Output>;

    fn sub(self, other: DataValue<TRhs>) -> Self::Output {
        self.combine(other, T::sub)
    }
}

impl<T, TRhs> Mul<DataValue<TRhs>> for DataValue<T>
where
    T: Mul<TRhs>,
{
    type Output = DataValue<T::Output>;

    fn mul(self, other: DataValue<TRhs>) -> Self::Output {
        self.combine(other, T::mul)
    }
}

impl<T, TRhs> Div<DataValue<TRhs>> for DataValue<T>
where
    T: Div<TRhs>,
{
    type Output = DataValue<T::Output>;

    fn div(self, other: DataValue<TRhs>) -> Self::Output {
        self.combine(other, T::div)
    }
}

impl<T, TRhs> BitAnd<DataValue<TRhs>> for DataValue<T>
where
    T: BitAnd<TRhs>,
{
    type Output = DataValue<T::Output>;

    fn bitand(self, other: DataValue<TRhs>) -> Self::Output {
        self.combine(other, T::bitand)
    }
}

impl<T, TRhs> BitOr<DataValue<TRhs>> for DataValue<T>
where
    T: BitOr<TRhs>,
{
    type Output = DataValue<T::Output>;

    fn bitor(self, other: DataValue<TRhs>) -> Self::Output {
        self.combine(other, T::bitor)
    }
}

impl<T, TRhs> BitXor<DataValue<TRhs>> for DataValue<T>
where
    T: BitXor<TRhs>,
{
    type Output = DataValue<T::Output>;

    fn bitxor(self, other: DataValue<TRhs>) -> Self::Output {
        self.combine(other, T::bitxor)
    }
}

impl<T, TRhs> Shr<DataValue<TRhs>> for DataValue<T>
where
    T: Shr<TRhs>,
{
    type Output = DataValue<T::Output>;

    fn shr(self, other: DataValue<TRhs>) -> Self::Output {
        self.combine(other, T::shr)
    }
}

impl<T, TRhs> Shl<DataValue<TRhs>> for DataValue<T>
where
    T: Shl<TRhs>,
{
    type Output = DataValue<T::Output>;

    fn shl(self, other: DataValue<TRhs>) -> Self::Output {
        self.combine(other, T::shl)
    }
}

impl<T> Not for DataValue<T>
where
    T: Not,
{
    type Output = DataValue<T::Output>;

    fn not(self) -> Self::Output {
        self.map(T::not)
    }
}

impl DataWord {
    pub fn overflowing_add(self, other: DataWord) -> (DataWord, bool) {
        self.overflowing_operation(other, UWord::overflowing_add)
    }

    pub fn overflowing_sub(self, other: DataWord) -> (DataWord, bool) {
        self.overflowing_operation(other, UWord::overflowing_sub)
    }

    pub fn overflowing_mul(self, other: DataWord) -> (DataWord, bool) {
        self.overflowing_operation(other, UWord::overflowing_mul)
    }

    pub fn overflowing_div(self, other: DataWord) -> (DataWord, bool) {
        self.overflowing_operation(other, UWord::overflowing_div)
    }

    pub fn overflowing_shl(self, other: DataWord) -> (DataWord, bool) {
        self.overflowing_operation(other, |a, b| a.overflowing_shl(b as u32))
    }

    pub fn overflowing_shr(self, other: DataWord) -> (DataWord, bool) {
        self.overflowing_operation(other, |a, b| a.overflowing_shr(b as u32))
    }
}

impl<T> Display for DataValue<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_reference {
            write!(f, "[R]")?;
        }
        write!(f, "{}", self.value)?;
        Ok(())
    }
}

impl<T> UpperHex for DataValue<T>
where
    T: UpperHex,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_reference {
            write!(f, "[R]")?;
        }
        write!(f, "{:02X}", self.value)?;
        Ok(())
    }
}

impl Interpreter {
    fn ip_reader(&mut self) -> InterpreterInstructionPointerReader {
        InterpreterInstructionPointerReader {
            memory: &self.memory,
            cpu_state: &mut self.cpu_state,
        }
    }

    fn step(&mut self) -> Result<bool> {
        let previous_ip = self.cpu_state.instruction_pointer;
        let opcode = Opcode::decode(&mut self.ip_reader())?;
        //println!("LAKESIS | {:016X} {}", previous_ip, opcode);

        match opcode.instruction {
            Instruction::NoOperation => {}

            Instruction::Move => {
                self.ensure_operands(&opcode, 2)?;
                let value = self.read(&opcode.operands[0])?;
                self.write_with_flags(&opcode.operands[1], value)?;
            }

            Instruction::Add => self.combine_with_carry(&opcode, DataWord::overflowing_add)?,
            Instruction::Subtract => {
                self.reverse_combine_with_carry(&opcode, DataWord::overflowing_sub)?
            }
            Instruction::Multiply => self.combine_with_carry(&opcode, DataWord::overflowing_mul)?,
            Instruction::Divide => {
                self.reverse_combine_with_carry(&opcode, DataWord::overflowing_div)?
            }

            Instruction::BitwiseAnd => self.combine(&opcode, DataWord::bitand)?,
            Instruction::BitwiseOr => self.combine(&opcode, DataWord::bitor)?,
            Instruction::BitwiseXor => self.combine(&opcode, DataWord::bitxor)?,

            Instruction::BitwiseNot => {
                self.ensure_operands(&opcode, 1)?;
                let result = !self.read(&opcode.operands[0])?;
                self.write_with_flags(&opcode.operands[0], result)?;
            }

            Instruction::ShiftLeft => {
                self.combine_with_carry(&opcode, DataWord::overflowing_shl)?
            }
            Instruction::ShiftRight => {
                self.combine_with_carry(&opcode, DataWord::overflowing_shr)?
            }

            Instruction::Compare => {
                self.ensure_operands(&opcode, 2)?;
                let value1 = self.read(&opcode.operands[0])?.value;
                let value2 = self.read(&opcode.operands[1])?.value;

                self.cpu_state.zero_flag = value1 == value2;
                self.cpu_state.carry_flag = value1 >= value2;
            }

            Instruction::Jump => self.jump(&opcode)?,
            Instruction::JumpEqual => self.conditional_jump(&opcode, Some(true), None)?,
            Instruction::JumpNotEqual => self.conditional_jump(&opcode, Some(false), None)?,
            Instruction::JumpGreater => self.conditional_jump(&opcode, Some(false), Some(true))?,
            Instruction::JumpGreaterEqual => self.conditional_jump(&opcode, None, Some(true))?,
            Instruction::JumpLess => self.conditional_jump(&opcode, None, Some(false))?,
            Instruction::JumpLessEqual => {
                // We don't use self.conditional_jump because it doesn't support 'or', only 'and' for the flags
                if self.cpu_state.zero_flag || !self.cpu_state.carry_flag {
                    self.jump(&opcode)?;
                }
            }

            Instruction::Call => {
                self.ensure_operands(&opcode, 1)?;
                let addr = self.read(&opcode.operands[0])?.value;

                self.push_stack(DataWord {
                    value: self.cpu_state.instruction_pointer.0,
                    is_reference: true,
                })?;
                self.cpu_state.instruction_pointer = Wrapping(addr);
            }

            Instruction::Return => {
                self.ensure_operands(&opcode, 0)?;

                let addr = self.pop_stack()?;
                if !addr.is_reference {
                    return Err(Error::new("Tried to return from a non-reference data word"));
                }

                self.cpu_state.instruction_pointer = Wrapping(addr.value);
            }

            Instruction::Push => {
                self.ensure_operands(&opcode, 1)?;
                let value = self.read(&opcode.operands[0])?;
                self.push_stack(value)?;
            }

            Instruction::Pop => {
                self.ensure_operands(&opcode, 1)?;
                let value = self.pop_stack()?;
                self.write(&opcode.operands[0], value)?;
            }

            Instruction::New => {
                self.ensure_operands(&opcode, 2)?;
                let size = self.read(&opcode.operands[0])?.value;

                let addr = DataWord {
                    value: self.memory.allocate(size, None)?,
                    is_reference: true,
                };
                self.write(&opcode.operands[1], addr)?;
            }

            Instruction::GarbageCollector => self.memory.force_garbage_collection()?,

            Instruction::Reference => {
                self.ensure_operands(&opcode, 1)?;
                let mut value = self.read(&opcode.operands[0])?;
                value.is_reference = true;
                self.write(&opcode.operands[0], value)?;
            }

            Instruction::Unreference => {
                self.ensure_operands(&opcode, 1)?;
                let mut value = self.read(&opcode.operands[0])?;
                value.is_reference = false;
                self.write(&opcode.operands[0], value)?;
            }

            Instruction::CallNative => {
                self.ensure_operands(&opcode, 1)?;
                match self.read(&opcode.operands[0])?.value {
                    0 => self.native_print()?,
                    1 => self.native_random()?,
                    2 => self.native_sleep()?,
                    _ => unimplemented!(),
                }
            }

            Instruction::DebugCpu => {
                self.ensure_operands(&opcode, 1)?;
                let num = self.read(&opcode.operands[0])?;

                println!("DEBUGCPU | {} | {}", num, self);
            }

            Instruction::DebugMemory => {
                self.ensure_operands(&opcode, 2)?;
                let addr = self.read(&opcode.operands[0])?.value;
                let len = self.read(&opcode.operands[1])?.value;
                let data = self.memory.get(addr, len)?;

                print!("DEBUGMEM | 0x{:X} | ", addr);

                let mut i = 0;
                for byte in data {
                    i += 1;
                    print!("{:02X} ", byte);

                    if i % WORD_BYTE_SIZE == 0 {
                        print!("  ");
                    }
                }

                println!()
            }

            Instruction::Halt => return Ok(false),
        };

        Ok(true)
    }

    fn ensure_operands(&self, op: &Opcode, expected_operands: usize) -> VoidResult {
        if op.operands.len() != expected_operands {
            Err(Error::new(&format!(
                "Expected {} operands, found only {}",
                expected_operands,
                op.operands.len()
            )))
        } else {
            Ok(())
        }
    }

    fn combine(
        &mut self,
        opcode: &Opcode,
        operation: impl FnOnce(DataWord, DataWord) -> DataWord,
    ) -> VoidResult {
        self.ensure_operands(&opcode, 2)?;
        let value1 = self.read(&opcode.operands[0])?;
        let value2 = self.read(&opcode.operands[1])?;
        let result = operation(value1, value2);

        self.write_with_flags(&opcode.operands[1], result)
    }

    fn reverse_combine_with_carry(
        &mut self,
        opcode: &Opcode,
        operation: impl FnOnce(DataWord, DataWord) -> (DataWord, bool),
    ) -> VoidResult {
        self.combine_with_carry(opcode, |a, b| operation(b, a))
    }

    fn combine_with_carry(
        &mut self,
        opcode: &Opcode,
        operation: impl FnOnce(DataWord, DataWord) -> (DataWord, bool),
    ) -> VoidResult {
        let mut carry = false;
        self.combine(opcode, |a, b| {
            let (result, carry_inner) = operation(a, b);
            carry = carry_inner;
            result
        })?;
        self.cpu_state.carry_flag = carry;

        Ok(())
    }

    fn read(&self, op: &Operand) -> Result<DataWord> {
        match op {
            Operand::Immediate(v) => Ok(DataValue {
                value: *v as UWord,
                is_reference: false,
            }),

            Operand::Register(i) => Ok(self.cpu_state.registers[*i as usize]),

            _ => {
                let addr = self.get_effective_address(op)?;
                self.memory.get_data_word(addr)
            }
        }
    }

    fn write(&mut self, op: &Operand, value: DataWord) -> VoidResult {
        match op {
            Operand::Immediate(_) => {
                Err(Error::new("Immediate value can't be used as a destination"))
            }

            Operand::Register(i) => {
                self.cpu_state.registers[*i as usize] = value;
                Ok(())
            }

            _ => {
                let addr = self.get_effective_address(op)?;
                self.memory.set_data_word(addr, value)
            }
        }
    }

    fn write_with_flags(&mut self, op: &Operand, value: DataWord) -> VoidResult {
        self.write(op, value)?;
        self.cpu_state.carry_flag = false;
        self.cpu_state.zero_flag = value.value == 0;
        Ok(())
    }

    fn get_effective_address(&self, op: &Operand) -> Result<UWord> {
        match op {
            Operand::Reference { register, offset } => {
                let base_addr = self.cpu_state.registers[*register as usize].expect_reference()?;
                let (addr, _) = base_addr.overflowing_add(*offset as UWord);
                Ok(addr)
            }

            Operand::Stack(offset) => {
                let base_addr = self.cpu_state.stack_pointer;
                let addr = base_addr + Wrapping(*offset as UWord);
                Ok(addr.0)
            }

            _ => panic!(
                "get_effective_address can only be called with a reference or stack reference"
            ),
        }
    }

    fn conditional_jump(
        &mut self,
        opcode: &Opcode,
        zero_flag: Option<bool>,
        carry_flag: Option<bool>,
    ) -> VoidResult {
        let zero_matches = match zero_flag {
            None => true,
            Some(expected) => self.cpu_state.zero_flag == expected,
        };

        let carry_matches = match carry_flag {
            None => true,
            Some(expected) => self.cpu_state.carry_flag == expected,
        };

        if zero_matches && carry_matches {
            self.jump(opcode)
        } else {
            Ok(())
        }
    }

    fn jump(&mut self, opcode: &Opcode) -> VoidResult {
        self.ensure_operands(&opcode, 1)?;
        let addr = self.read(&opcode.operands[0])?.value;
        self.cpu_state.instruction_pointer = Wrapping(addr);
        Ok(())
    }

    fn push_stack(&mut self, value: DataWord) -> VoidResult {
        //println!("LAKESIS | Push@{:X}: {:X}", self.cpu_state.stack_pointer, value);

        self.memory
            .set_data_word(self.cpu_state.stack_pointer.0, value)?;
        self.cpu_state.stack_pointer -= Wrapping(WORD_BYTE_SIZE);

        Ok(())
    }

    fn pop_stack(&mut self) -> Result<DataWord> {
        self.cpu_state.stack_pointer += Wrapping(WORD_BYTE_SIZE);
        let result = self.memory.get_data_word(self.cpu_state.stack_pointer.0)?;

        //println!("LAKESIS | Pop@{:X}: {:X}", self.cpu_state.stack_pointer, result);
        Ok(result)
    }

    fn read_native_parameter(&self, parameter_index: UWord) -> Result<DataWord> {
        let byte_offset = Wrapping(parameter_index + 1) * Wrapping(WORD_BYTE_SIZE);
        let address = self.cpu_state.stack_pointer + byte_offset;

        self.memory.get_data_word(address.0)
    }

    fn native_print(&mut self) -> VoidResult {
        let string_len = self.read_native_parameter(0)?;
        let string_base_addr = self.read_native_parameter(1)?;

        if !string_base_addr.is_reference {
            return Err(Error::new("Base address provided isn't a reference"));
        }

        let string = self.memory.get(string_base_addr.value, string_len.value)?;

        let mut i = 0;
        let mut param_index = 2;
        while i < string.len() {
            if string[i] == b'%' {
                i += 1;
                if i >= string.len() {
                    return Err(Error::new("Unterminated format string placeholder"));
                }

                // TODO: This is ugly
                if string[i] == b'%' {
                    print!("%");
                } else if string[i] == b'd' {
                    let param = self.read_native_parameter(param_index)?.value as IWord;
                    param_index += 1;

                    print!("{}", param);
                } else if string[i] == b'u' {
                    let param = self.read_native_parameter(param_index)?.value;
                    param_index += 1;

                    print!("{}", param);
                } else if string[i] == b's' {
                    let param_len = self.read_native_parameter(param_index)?.value;
                    param_index += 1;
                    let param = self.read_native_parameter(param_index)?;
                    param_index += 1;

                    if !param.is_reference {
                        return Err(Error::new("Tried to print a non-reference as a string"));
                    }

                    let param_utf8 = self.memory.get(param.value, param_len)?;
                    let param_str = String::from_utf8_lossy(param_utf8);
                    print!("{}", param_str);
                }
            } else {
                print!("{}", string[i] as char);
            }

            i += 1;
        }

        Ok(())
    }

    fn native_random(&mut self) -> VoidResult {
        self.cpu_state.registers[0] = DataWord {
            value: rand::random(),
            is_reference: false,
        };
        Ok(())
    }

    fn native_sleep(&self) -> VoidResult {
        let millis = self.read_native_parameter(0)?.value;
        thread::sleep(Duration::from_millis(millis));
        Ok(())
    }
}

impl Display for Interpreter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for i in 0..REGISTER_NUM {
            write!(f, "R{}={:02X} ", i, self.cpu_state.registers[i])?;
        }

        write!(f, "IP={:02X} ", self.cpu_state.instruction_pointer)?;
        write!(f, "SP={:02X} ", self.cpu_state.stack_pointer)?;

        if self.cpu_state.carry_flag {
            write!(f, "C");
        } else {
            write!(f, "c");
        }

        if self.cpu_state.zero_flag {
            write!(f, "Z");
        } else {
            write!(f, "z");
        }

        Ok(())
    }
}

impl Read for InterpreterInstructionPointerReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let data = self
            .memory
            .get(self.cpu_state.instruction_pointer.0, buf.len() as UWord)?;
        buf.copy_from_slice(data);
        self.cpu_state.instruction_pointer += Wrapping(buf.len() as UWord);
        Ok(buf.len())
    }
}

pub fn run(reader: &mut impl Read) -> VoidResult {
    let mut interpreter = Interpreter {
        cpu_state: CpuState::default(),
        memory: Memory::new(),
    };

    let mut program_data = Vec::new();
    reader.read_to_end(&mut program_data)?;

    let mut aligned_len = program_data.len() as UWord;
    while aligned_len % WORD_BYTE_SIZE != 0 {
        aligned_len += 1;
    }

    if interpreter.memory.allocate(aligned_len, Some(0))? != 0 {
        return Err(Error::new("Unable to allocate program data at address 0"));
    }

    interpreter.memory.set(0, &program_data)?;

    let stack_base = interpreter.memory.allocate(STACK_SIZE, None)?;
    interpreter.cpu_state.stack_pointer =
        Wrapping(stack_base) + Wrapping(STACK_SIZE) - Wrapping(WORD_BYTE_SIZE);

    //println!("LAKESIS | {}", interpreter);

    while interpreter.step()? {
        //println!("LAKESIS | {}", interpreter);
    }

    Ok(())
}
