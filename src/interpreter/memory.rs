use super::DataWord;
use crate::core::{Error, Result, UWord, VoidResult};
use std::convert::TryInto;
use std::io::{self, Read};

#[derive(Clone, Debug)]
pub struct Memory {
    // TODO: Implement actual memory management
    data: Vec<u8>,
    references: Vec<bool>,
}

impl Memory {
    pub fn new() -> Memory {
        Memory {
            data: Vec::new(),
            references: Vec::new(),
        }
    }

    pub fn reader_for(&self, addr: UWord) -> MemoryReader {
        MemoryReader::new(self, addr)
    }

    pub fn get(&self, addr: UWord, size: UWord) -> Result<&[u8]> {
        self.ensure_mapped(addr, size)?;

        let addr = addr as usize;
        let size = size as usize;

        Ok(&self.data[addr..addr + size])
    }

    pub fn set(&mut self, addr: UWord, data: &[u8]) -> VoidResult {
        self.ensure_mapped(addr, data.len() as UWord)?;

        let mut i = addr as usize;
        for &byte in data {
            self.data[i] = byte;
            i += 1;
        }
        Ok(())
    }

    pub fn is_reference(&self, addr: UWord) -> Result<bool> {
        Self::ensure_aligned(addr)?;
        Ok(self.references[addr as usize / 8])
    }

    pub fn set_reference(&mut self, addr: UWord, is_reference: bool) -> VoidResult {
        self.references[addr as usize / 8] = is_reference;
        Ok(())
    }

    pub fn get_word(&self, addr: UWord) -> Result<UWord> {
        Self::ensure_aligned(addr)?;
        Ok(UWord::from_le_bytes(
            self.get(addr, 8)?.try_into().expect("Invalid array size"),
        ))
    }

    pub fn set_word(&mut self, addr: UWord, value: UWord) -> VoidResult {
        Self::ensure_aligned(addr)?;
        self.set(addr, &value.to_le_bytes())?;
        Ok(())
    }

    pub fn get_data_word(&self, addr: UWord) -> Result<DataWord> {
        Ok(DataWord {
            value: self.get_word(addr)?,
            is_reference: self.is_reference(addr)?,
        })
    }

    pub fn set_data_word(&mut self, addr: UWord, value: DataWord) -> VoidResult {
        self.set_word(addr, value.value)?;
        self.set_reference(addr, value.is_reference)?;
        Ok(())
    }

    pub fn allocate(&mut self, size: UWord, preferred_base: Option<UWord>) -> Result<UWord> {
        if let Some(base) = preferred_base {
            Self::ensure_aligned(base)?;
            while (self.data.len() as UWord) < base {
                self.data.push(0);
            }
        }

        let base_addr = self.data.len() as UWord;

        for _ in 0..size {
            self.data.push(0);
        }

        while self.references.len() < self.data.len() / 8 {
            self.references.push(false);
        }

        Ok(base_addr)
    }

    pub fn force_garbage_collection(&mut self) -> VoidResult {
        // Not implemented yet
        Ok(())
    }

    fn ensure_aligned(addr: UWord) -> VoidResult {
        if addr % 8 != 0 {
            Err(Error::new(&format!(
                "Address {:016X} isn't word-aligned",
                addr
            )))
        } else {
            Ok(())
        }
    }

    fn ensure_mapped(&self, addr: UWord, size: UWord) -> VoidResult {
        if size == 0 {
            return Ok(());
        }

        let addr = addr as usize;
        let size = size as usize;

        if addr + size - 1 >= self.data.len() {
            return Err(Error::new(&format!(
                "Address range {:016X}-{:016X} isn't mapped",
                addr,
                addr + size
            )));
        }

        Ok(())
    }
}

pub struct MemoryReader<'a> {
    memory: &'a Memory,
    base_addr: UWord,
    addr: UWord,
}

impl MemoryReader<'_> {
    fn new(memory: &Memory, addr: UWord) -> MemoryReader {
        MemoryReader {
            memory,
            base_addr: addr,
            addr,
        }
    }

    pub fn offset(&self) -> UWord {
        self.addr - self.base_addr
    }
}

impl Read for MemoryReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut i = 0;
        for &byte in self.memory.get(self.addr, buf.len() as UWord)? {
            buf[i] = byte;
            self.addr += 1;
            i += 1;
        }

        Ok(buf.len())
    }
}
