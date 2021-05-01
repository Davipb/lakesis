use super::DataWord;
use crate::core::{Error, Result, UWord, VoidResult, MAX_MEMORY_SIZE, WORD_BYTE_SIZE};
use bitvec::prelude::*;
use bitvec::ptr::{Const, Mut};
use bitvec::slice::BitSliceIndex;
use bytesize::ByteSize;
use std::alloc;
use std::alloc::Layout;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Read};
use std::ops::{Add, Div, Mul, Rem, Sub};
use std::ptr;
use std::slice;

const VIRTUAL_PAGE_SIZE: UWord = 1024;

type AllocationId = u64;
type VirtualAddressBlockId = u64;
type HeapRegionId = u64;

#[derive(Clone, Debug)]
pub struct Memory {
    virtual_mapper: VirtualAddressMapper,
    regions: HeapRegions,
    allocations: IdHashMap<Allocation>,
    heap: Vec<u8>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Allocation {
    id: AllocationId,
    base: usize,
    length: usize,
    is_collectible: bool,
    name: Option<String>,
    virtual_block: VirtualAddressBlockId,
    region: HeapRegionId,
}

impl Memory {
    pub fn new() -> Memory {
        Memory {
            virtual_mapper: VirtualAddressMapper::new(),
            allocations: IdHashMap::new(),
            regions: HeapRegions::new(MAX_MEMORY_SIZE),
            heap: vec![0; MAX_MEMORY_SIZE],
        }
    }

    pub fn reader_for(&self, addr: UWord) -> MemoryReader {
        MemoryReader::new(self, addr)
    }

    pub fn get(&self, addr: UWord, size: UWord) -> Result<&[u8]> {
        Ok(self.addr_to_slice(addr, size)?)
    }

    pub fn set(&mut self, addr: UWord, data: &[u8]) -> VoidResult {
        let slice = self.addr_to_mut_slice(addr, data.len() as UWord)?;
        slice.copy_from_slice(data);

        Ok(())
    }

    pub fn is_reference(&self, addr: UWord) -> Result<bool> {
        Ok(*self.addr_to_reference_ptr(addr)?)
    }

    pub fn set_reference(&mut self, addr: UWord, is_reference: bool) -> VoidResult {
        *self.addr_to_reference_ptr_mut(addr)? = is_reference;
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
        name: Option<&str>,
    ) -> Result<UWord> {
        let allocation_id = self.allocations.peek_next_id();

        let (base, region_id) = self.regions.allocate(data_size as usize, allocation_id)?;

        let (addr, virtual_block_id) =
            self.virtual_mapper
                .map(data_size, allocation_id, preferred_base)?;

        self.allocations.insert(Allocation {
            id: 0,
            base: base,
            length: data_size as usize,
            is_collectible,
            name: name.map(ToOwned::to_owned),
            region: region_id,
            virtual_block: virtual_block_id,
        });

        Ok(addr)
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

    fn addr_to_allocation(&self, addr: UWord) -> Result<(&Allocation, usize)> {
        let (allocation_id, offset) = self.virtual_mapper.translate(addr)?;

        let allocation = self
            .allocations
            .get(allocation_id)
            .expect("Virtual address pointed to non-existent allocation");

        Ok((allocation, offset))
    }

    fn addr_to_indices(&self, addr: UWord, size: UWord) -> Result<(usize, usize)> {
        let (allocation, offset) = self.addr_to_allocation(addr)?;

        let readable_len = allocation.length - offset;
        if readable_len < size as usize {
            return Err(Error::new(&format!(
                "Tried to access {} bytes but only {} are available",
                size, readable_len
            )));
        }

        let start = allocation.base + offset;
        let end = start + size as usize;

        Ok((start, end))
    }

    fn addr_to_slice(&self, addr: UWord, size: UWord) -> Result<&[u8]> {
        let (start, end) = self.addr_to_indices(addr, size)?;

        Ok(&self.heap[start..end])
    }

    fn addr_to_mut_slice(&mut self, addr: UWord, size: UWord) -> Result<&mut [u8]> {
        let (start, end) = self.addr_to_indices(addr, size)?;

        Ok(&mut self.heap[start..end])
    }

    fn addr_to_reference_indices(&self, addr: UWord) -> Result<(usize, usize, usize)> {
        if addr % WORD_BYTE_SIZE != 0 {
            return Err(Error::new("Address isn't byte-aligned"));
        }

        let (allocation, byte_offset) = self.addr_to_allocation(addr)?;
        let word_offset = byte_offset / WORD_BYTE_SIZE as usize;

        let start = allocation.base + allocation.length;
        let end = start + bitfield_len(allocation.length);

        Ok((start, end, word_offset))
    }

    fn addr_to_reference_ptr_mut(&mut self, addr: UWord) -> Result<BitRef<Mut, Lsb0, u8>> {
        let (start, end, offset) = self.addr_to_reference_indices(addr)?;

        let slice = &mut self.heap[start..end];
        let bitfield = slice.view_bits_mut();
        Ok(bitfield
            .get_mut(offset)
            .expect("Unable to read reference bitfield"))
    }

    fn addr_to_reference_ptr(&self, addr: UWord) -> Result<BitRef<Const, Lsb0, u8>> {
        let (start, end, offset) = self.addr_to_reference_indices(addr)?;

        let slice = &self.heap[start..end];
        let bitfield = slice.view_bits();
        Ok(bitfield
            .get(offset)
            .expect("Unable to read reference bitfield"))
    }
}

impl Display for Memory {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Allocations:")?;

        let mut sorted_allocations: Vec<&Allocation> = self.allocations.iter().collect();
        sorted_allocations.sort_unstable_by_key(|x| x.base);
        for allocation in sorted_allocations {
            write!(f, "\n  {}", allocation)?;
        }

        write!(f, "\n{}", self.virtual_mapper)?;
        write!(f, "\n{}", self.regions)?;

        Ok(())
    }
}

impl Display for Allocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "A{:04} {} {:08X} {:>10} R{:04} V{:04} {}",
            self.id,
            if self.is_collectible { " " } else { "!" },
            self.base,
            ByteSize(self.length as u64),
            self.region,
            self.virtual_block,
            match &self.name {
                None => "",
                Some(s) => s,
            }
        )
    }
}

impl HeapRegionState {
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
        let read = self.memory.get(self.addr, buf.len() as UWord)?;
        buf.copy_from_slice(read);

        self.addr += buf.len() as UWord;
        Ok(buf.len())
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
struct VirtualAddressMapping {
    block: VirtualAddressBlockId,
    offset: usize,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
struct VirtualAddressBlock {
    id: VirtualAddressBlockId,
    allocation: AllocationId,
    base: UWord,
    size: UWord,
}

#[derive(Clone, Debug)]
struct VirtualAddressMapper {
    blocks: IdHashMap<VirtualAddressBlock>,
    mappings: HashMap<UWord, VirtualAddressMapping>,
    next_address: UWord,
}

impl VirtualAddressMapper {
    fn new() -> VirtualAddressMapper {
        VirtualAddressMapper {
            blocks: IdHashMap::new(),
            mappings: HashMap::new(),
            next_address: 0,
        }
    }

    fn map(
        &mut self,
        size: UWord,
        allocation: AllocationId,
        preferred_base: Option<UWord>,
    ) -> Result<(UWord, VirtualAddressBlockId)> {
        if let Some(base) = preferred_base {
            if base % VIRTUAL_PAGE_SIZE != 0 {
                return Err(Error::new(&format!(
                    "Requested base address {:08X} isn't page-aligned",
                    base
                )));
            }

            if base < self.next_address {
                return Err(Error::new(&format!(
                    "Unable to meet requested base address {:08X}",
                    base
                )));
            }

            self.next_address = base;
        }

        let base_addr = self.next_address;

        let block_id = self.blocks.insert(VirtualAddressBlock {
            id: 0,
            base: base_addr,
            size,
            allocation,
        });

        for page in 0..=(size / VIRTUAL_PAGE_SIZE) {
            self.mappings.insert(
                self.next_address,
                VirtualAddressMapping {
                    block: block_id,
                    offset: page as usize * VIRTUAL_PAGE_SIZE as usize,
                },
            );
            self.next_address += VIRTUAL_PAGE_SIZE;
        }

        Ok((base_addr, block_id))
    }

    fn translate(&self, addr: UWord) -> Result<(AllocationId, usize)> {
        let aligned_addr = round_down_to(addr, VIRTUAL_PAGE_SIZE);
        let alignment_offset = addr as usize - aligned_addr as usize;

        let mapping = self.mappings.get(&aligned_addr).ok_or_else(|| {
            Error::new(&format!(
                "Tried to access unmapped memory address {:08X}",
                addr
            ))
        })?;

        let block = self
            .blocks
            .get(mapping.block)
            .expect("Mapping pointed to an invalid block");

        Ok((block.allocation, mapping.offset + alignment_offset))
    }
}

impl VirtualAddressBlock {
    fn end(&self) -> UWord {
        self.base + self.size
    }
}

impl Display for VirtualAddressMapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut sorted_blocks: Vec<&VirtualAddressBlock> = self.blocks.iter().collect();
        sorted_blocks.sort_unstable_by_key(|x| x.base);

        write!(f, "Virtual Address Blocks")?;
        for block in sorted_blocks {
            write!(f, "\n  {}", block)?;
        }

        Ok(())
    }
}

impl Display for VirtualAddressBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "V{:04} {:08X} - {:08X} {:>10} A{:04}",
            self.id,
            self.base,
            self.end(),
            ByteSize(self.size),
            self.allocation
        )
    }
}

#[derive(Clone, Debug)]
struct HeapRegions {
    map: IdHashMap<HeapRegion>,
    in_order: Vec<HeapRegionId>,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
struct HeapRegion {
    id: HeapRegionId,
    state: HeapRegionState,
    base: usize,
    length: usize,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
enum HeapRegionState {
    Free,
    Used(AllocationId),
}

impl HeapRegions {
    fn new(size: usize) -> HeapRegions {
        let mut regions = HeapRegions {
            map: IdHashMap::new(),
            in_order: Vec::with_capacity(1),
        };

        let id = regions.map.insert(HeapRegion {
            id: 0,
            state: HeapRegionState::Free,
            base: 0,
            length: size,
        });

        regions.in_order.push(id);

        regions
    }

    fn allocate(
        &mut self,
        data_size: usize,
        allocation: AllocationId,
    ) -> Result<(usize, HeapRegionId)> {
        let total_size = data_size as usize + bitfield_len(data_size as usize);

        let (index, region) = self
            .in_order
            .iter()
            .map(|id| {
                self.map
                    .get(*id)
                    .expect("In-order vector pointed to non-existent region")
            })
            .enumerate()
            .filter(|(_, x)| x.state.is_free())
            .find(|(_, x)| x.length >= total_size)
            .ok_or_else(|| Error::new("Out of memory"))?;

        let region_id = region.id;

        if region.length > total_size {
            let new_region_id = self.map.insert(HeapRegion {
                id: 0,
                state: HeapRegionState::Free,
                base: region.base + total_size,
                length: region.length - total_size,
            });
            self.in_order.insert(index + 1, new_region_id);
        }

        let region = self.map.get_mut(region_id).unwrap();
        region.length = total_size;
        region.state = HeapRegionState::Used(allocation);

        Ok((region.base, region.id))
    }
}

impl Display for HeapRegions {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut sorted_regions: Vec<&HeapRegion> = self.map.iter().collect();
        sorted_regions.sort_unstable_by_key(|x| x.base);

        write!(f, "Heap Regions")?;
        for region in sorted_regions {
            write!(f, "\n  {}", region)?;
        }

        Ok(())
    }
}

impl Display for HeapRegion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "R{:04} {:08X} {:>10} {}",
            self.id,
            self.base,
            ByteSize(self.length as u64),
            self.state
        )
    }
}

impl Display for HeapRegionState {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            HeapRegionState::Free => write!(f, "Free"),
            HeapRegionState::Used(id) => write!(f, "A{:04}", *id),
        }
    }
}

trait IdStruct {
    fn get_id(&self) -> u64;
    fn set_id(&mut self, id: u64);
}

impl IdStruct for Allocation {
    fn get_id(&self) -> u64 {
        self.id
    }
    fn set_id(&mut self, id: u64) {
        self.id = id
    }
}

impl IdStruct for VirtualAddressBlock {
    fn get_id(&self) -> u64 {
        self.id
    }
    fn set_id(&mut self, id: u64) {
        self.id = id
    }
}

impl IdStruct for HeapRegion {
    fn get_id(&self) -> u64 {
        self.id
    }
    fn set_id(&mut self, id: u64) {
        self.id = id
    }
}

#[derive(Clone, Debug)]
struct IdHashMap<T>
where
    T: IdStruct,
{
    next_id: u64,
    map: HashMap<u64, T>,
}

impl<T> IdHashMap<T>
where
    T: IdStruct,
{
    fn new() -> IdHashMap<T> {
        IdHashMap {
            next_id: 1,
            map: HashMap::new(),
        }
    }

    fn peek_next_id(&self) -> u64 {
        self.next_id
    }

    fn insert(&mut self, mut item: T) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        item.set_id(id);
        self.map.insert(id, item);

        id
    }

    fn get(&self, id: u64) -> Option<&T> {
        self.map.get(&id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut T> {
        self.map.get_mut(&id)
    }

    fn remove(&mut self, id: u64) {
        self.map.remove(&id);
    }

    fn iter(&self) -> std::collections::hash_map::Values<u64, T> {
        self.map.values()
    }
}

impl<'a, T> IntoIterator for &'a IdHashMap<T>
where
    T: IdStruct,
{
    type Item = &'a T;
    type IntoIter = std::collections::hash_map::Values<'a, u64, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

fn bitfield_len(data_len: usize) -> usize {
    data_len / WORD_BYTE_SIZE as usize / std::mem::size_of::<u8>()
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
