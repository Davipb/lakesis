use super::DataWord;
use crate::core::{Error, Result, UWord, VoidResult, MAX_MEMORY_SIZE, WORD_BYTE_SIZE};
use bitvec::prelude::*;
use bitvec::ptr::Mut;
use bitvec::slice::BitSliceIndex;
use std::alloc;
use std::alloc::Layout;
use std::collections::HashMap;
use std::convert::TryInto;
use std::io::{self, Read};
use std::ops::{Add, Div, Mul, Rem, Sub};
use std::ptr;
use std::slice;
use std::usize::MAX;

const VIRTUAL_PAGE_SIZE: UWord = 1024;
const HEAP_ALIGN: usize = 1024;

type HeapRegionId = u64;

#[derive(Clone, Debug)]
pub struct Memory {
    virtual_addresses: HashMap<UWord, VirtualAddressMapping>,
    next_virtual_address: UWord,

    regions: HashMap<HeapRegionId, HeapRegion>,
    next_region_id: HeapRegionId,

    heap: *mut u8,
    heap_size: usize,
    blocks: Vec<ContiguousBlock>,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
struct VirtualAddressMapping {
    region: HeapRegionId,
    offset: usize,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
struct HeapRegion {
    id: HeapRegionId,
    base: usize,
    length: usize,
    is_collectible: bool,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
struct ContiguousBlock {
    state: ContiguousBlockState,
    base: usize,
    length: usize,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
enum ContiguousBlockState {
    Free,
    Used(HeapRegionId),
}

impl Memory {
    pub fn new() -> Memory {
        Memory {
            virtual_addresses: HashMap::new(),
            next_virtual_address: 0,

            regions: HashMap::new(),
            next_region_id: 0,

            heap: unsafe {
                alloc::alloc(Layout::from_size_align(MAX_MEMORY_SIZE, HEAP_ALIGN).unwrap())
            },
            heap_size: MAX_MEMORY_SIZE,
            blocks: vec![ContiguousBlock {
                state: ContiguousBlockState::Free,
                base: 0,
                length: MAX_MEMORY_SIZE,
            }],
        }
    }

    pub fn reader_for(&self, addr: UWord) -> MemoryReader {
        MemoryReader::new(self, addr)
    }

    pub fn get(&self, addr: UWord, size: UWord) -> Result<&[u8]> {
        let ptr = self.addr_to_ptr(addr, size)?;

        unsafe { Ok(slice::from_raw_parts(ptr, size as usize)) }
    }

    pub fn set(&mut self, addr: UWord, data: &[u8]) -> VoidResult {
        let ptr = self.addr_to_ptr(addr, data.len() as UWord)?;

        unsafe { ptr::copy(data.as_ptr(), ptr, data.len()) }

        Ok(())
    }

    pub fn is_reference(&self, addr: UWord) -> Result<bool> {
        Ok(*self.addr_to_reference_ptr(addr)?)
    }

    pub fn set_reference(&mut self, addr: UWord, is_reference: bool) -> VoidResult {
        *self.addr_to_reference_ptr(addr)? = is_reference;
        Ok(())
    }

    pub fn get_word(&self, addr: UWord) -> Result<UWord> {
        Self::ensure_aligned(addr)?;
        Ok(UWord::from_le_bytes(
            self.get(addr, WORD_BYTE_SIZE)?
                .try_into()
                .expect("Invalid array size"),
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

    pub fn allocate(
        &mut self,
        data_size: UWord,
        preferred_base: Option<UWord>,
        is_collectible: bool,
    ) -> Result<UWord> {
        if let Some(base) = preferred_base {
            if base % VIRTUAL_PAGE_SIZE != 0 {
                return Err(Error::new(&format!(
                    "Requested base address {:08X} isn't page-aligned",
                    base
                )));
            }

            if base < self.next_virtual_address {
                return Err(Error::new(&format!(
                    "Unable to meet requested base address {:08X}",
                    base
                )));
            }

            self.next_virtual_address = base;
        }

        let total_size = data_size as usize + Self::bitfield_len(data_size as usize);

        let result = self
            .blocks
            .iter_mut()
            .enumerate()
            .filter(|(_, b)| b.state.is_free())
            .find(|(_, b)| b.length >= total_size);

        let (index, block) = match result {
            None => return Err(Error::new("Out of memory")),
            Some(x) => x,
        };

        self.regions.insert(
            self.next_region_id,
            HeapRegion {
                id: self.next_region_id,
                base: block.base,
                length: data_size as usize,
                is_collectible,
            },
        );
        let region_id = self.next_region_id;
        self.next_region_id += 1;

        block.state = ContiguousBlockState::Used(region_id);

        if block.length > total_size {
            let new_block = ContiguousBlock {
                state: ContiguousBlockState::Free,
                base: block.base + total_size,
                length: block.length - total_size,
            };
            block.length = total_size;
            self.blocks.insert(index + 1, new_block);
        }

        let base_addr = self.next_virtual_address;

        for page in 0..=(data_size / VIRTUAL_PAGE_SIZE) {
            self.virtual_addresses.insert(
                self.next_virtual_address,
                VirtualAddressMapping {
                    region: region_id,
                    offset: page as usize * VIRTUAL_PAGE_SIZE as usize,
                },
            );
            self.next_virtual_address += VIRTUAL_PAGE_SIZE;
        }

        Ok(base_addr)
    }

    pub fn force_garbage_collection(&mut self) -> VoidResult {
        // Not implemented yet
        Ok(())
    }

    fn ensure_aligned(addr: UWord) -> VoidResult {
        if addr % WORD_BYTE_SIZE != 0 {
            Err(Error::new(&format!(
                "Address {:016X} isn't word-aligned",
                addr
            )))
        } else {
            Ok(())
        }
    }

    fn addr_to_region(&self, addr: UWord) -> Result<(&HeapRegion, usize)> {
        let aligned_addr = round_down_to(addr, VIRTUAL_PAGE_SIZE);
        let alignment_offset = addr as usize - aligned_addr as usize;

        let mapping = self.virtual_addresses.get(&aligned_addr).ok_or_else(|| {
            Error::new(&format!(
                "Tried to access unmapped memory address {:08X}",
                addr
            ))
        })?;

        let region = self
            .regions
            .get(&mapping.region)
            .ok_or_else(|| Error::new(&format!("Heap region {} doesn't exist", mapping.region)))?;

        Ok((region, mapping.offset + alignment_offset))
    }

    fn addr_to_ptr(&self, addr: UWord, size: UWord) -> Result<*mut u8> {
        let (region, offset) = self.addr_to_region(addr)?;

        let readable_len = region.length - offset;
        if readable_len < size as usize {
            return Err(Error::new(&format!(
                "Tried to access {} bytes but only {} are available",
                size, readable_len
            )));
        }

        unsafe { Ok(self.heap.add(region.base).add(offset)) }
    }

    fn addr_to_reference_ptr(&self, addr: UWord) -> Result<BitRef<Mut, Lsb0, u8>> {
        if addr % WORD_BYTE_SIZE != 0 {
            return Err(Error::new("Address isn't byte-aligned"));
        }

        let (region, offset) = self.addr_to_region(addr)?;
        let word_offset = offset / WORD_BYTE_SIZE as usize;

        let bitfield_len = Self::bitfield_len(region.length);
        let bitfield_slice = unsafe {
            let start = self.heap.add(region.base).add(region.length);
            slice::from_raw_parts_mut(start, bitfield_len)
        };

        let bitfield = bitfield_slice.view_bits_mut();
        Ok(bitfield
            .get_mut(word_offset)
            .expect("Unable to read reference bitfield"))
    }

    fn bitfield_len(region_len: usize) -> usize {
        region_len / WORD_BYTE_SIZE as usize / std::mem::size_of::<u8>()
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        if self.heap.is_null() || self.heap_size == 0 {
            return;
        }

        unsafe {
            alloc::dealloc(
                self.heap,
                Layout::from_size_align(self.heap_size, HEAP_ALIGN)
                    .expect("Invalid layout when dealloc'ing heap"),
            )
        }
    }
}

impl ContiguousBlockState {
    fn is_free(&self) -> bool {
        match self {
            Self::Free => true,
            Self::Used(_) => false,
        }
    }

    fn is_used(&self) -> bool {
        !self.is_free()
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

fn round_down_to<T>(value: T, alignment: T) -> T
where
    T: Copy + Div<Output = T> + Mul<Output = T>,
{
    (value / alignment) * alignment
}

fn round_up_to<T>(value: T, alignment: T) -> T
where
    T: Copy + Add<Output = T> + Sub<Output = T> + Div<Output = T> + Mul<Output = T> + From<u8>,
{
    ((value + alignment - 1.into()) / alignment) * alignment
}
