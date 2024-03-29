use super::DataWord;
use crate::core::{
    Error, Result, UWord, VoidResult, INITIAL_MEMORY_SIZE, MAX_MEMORY_SIZE, WORD_BYTE_SIZE,
};
use bitvec::prelude::*;
use bitvec::ptr::{Const, Mut};
use bytesize::ByteSize;
use std::alloc;
use std::alloc::Layout;
use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::fmt::{self, Display, Formatter};
use std::hash::Hash;
use std::io::{self, Read};
use std::ops::{Add, Deref, DerefMut, Div, Index, IndexMut, Mul, Range, Sub};
use std::ptr;
use std::slice;

const VIRTUAL_PAGE_SIZE: UWord = 1024;

#[derive(Debug)]
pub struct Memory {
    virtual_mapper: VirtualAddressMapper,
    regions: HeapRegions,
    allocations: IdHashMap<Allocation>,
    heap: Heap,
}

impl Memory {
    pub fn new() -> Memory {
        Memory {
            virtual_mapper: VirtualAddressMapper::new(),
            allocations: IdHashMap::new(),
            regions: HeapRegions::new(INITIAL_MEMORY_SIZE),
            heap: Heap::new(INITIAL_MEMORY_SIZE),
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
        is_collectible: bool,
        gc_roots: &[DataWord],
        preferred_base: Option<UWord>,
        name: Option<&str>,
    ) -> Result<UWord> {
        let allocation_id = self.allocations.peek_next_id();

        let (start, region_id) = self.try_allocate_region(data_size, allocation_id, gc_roots)?;

        let (addr, virtual_block_id) =
            self.virtual_mapper
                .map(data_size, allocation_id, preferred_base)?;

        let allocation_id = self.allocations.insert(Allocation {
            id: Default::default(),
            start,
            data_length: data_size as usize,
            is_collectible,
            name: name.map(ToOwned::to_owned),
            region: region_id,
            virtual_block: virtual_block_id,
        });

        let allocation = self.allocations.get(allocation_id).unwrap();

        for x in &mut self.heap[allocation.bitfield_start()..allocation.bitfield_end()] {
            *x = 0;
        }

        Ok(addr)
    }

    pub fn force_garbage_collection(&mut self, gc_roots: &[DataWord]) -> VoidResult {
        let mut collectible = HashSet::with_capacity(self.allocations.len());
        let mut visited = HashSet::with_capacity(self.allocations.len());
        let mut next: Vec<UWord> = gc_roots
            .iter()
            .filter(|x| x.is_reference)
            .map(|x| x.value)
            .collect();

        for (&id, allocation) in self.allocations.entry_iter() {
            if allocation.is_collectible {
                collectible.insert(id);
            } else {
                let block = self
                    .virtual_mapper
                    .get(allocation.virtual_block)
                    .expect("Allocation pointed to non-existent virtual memory block");

                next.push(block.base);
            }
        }

        while let Some(addr) = next.pop() {
            let (allocation, _) = match self.addr_to_allocation(addr) {
                Ok(x) => x,
                Err(_) => continue,
            };

            if !visited.insert(allocation.id) {
                continue;
            }

            collectible.remove(&allocation.id);

            self.get_bitfield(allocation)
                .iter_ones()
                .map(|i| allocation.start + (i * WORD_BYTE_SIZE as usize))
                .map(|x| {
                    UWord::from_le_bytes(
                        self.heap[x..x + WORD_BYTE_SIZE as usize]
                            .try_into()
                            .expect("Invalid array size"),
                    )
                })
                .for_each(|x| next.push(x))
        }

        for id in collectible {
            //println!("LAKESIS | GC: Deallocating {}", id);
            self.deallocate(id)?;
        }

        //println!("LAKESIS | GC: Compacting memory");
        self.regions.compact(&mut self.heap);

        for allocation in self.allocations.iter_mut() {
            let region = self.regions.get(allocation.region).unwrap();
            allocation.start = region.base;
        }

        Ok(())
    }

    fn try_allocate_region(
        &mut self,
        data_size: UWord,
        allocation_id: AllocationId,
        gc_roots: &[DataWord],
    ) -> Result<(usize, HeapRegionId)> {
        match self.regions.allocate(data_size as usize, allocation_id) {
            HeapRegionAllocationResult::Success { base, id } => return Ok((base, id)),
            HeapRegionAllocationResult::Error(e) => return Err(e),
            HeapRegionAllocationResult::OutOfMemory => {}
        };

        // Not enough space left, try to free up memory by running the GC
        self.force_garbage_collection(gc_roots)?;

        match self.regions.allocate(data_size as usize, allocation_id) {
            HeapRegionAllocationResult::Success { base, id } => return Ok((base, id)),
            HeapRegionAllocationResult::Error(e) => return Err(e),
            HeapRegionAllocationResult::OutOfMemory => {}
        };

        // Still not enough space left, try to expand the heap

        let minimum_required = self.regions.used_bytes() + total_region_len(data_size as usize);
        if minimum_required > MAX_MEMORY_SIZE {
            self.report_out_of_memory(data_size);
            return Err(Error::new("Out of memory"));
        }

        let mut new_heap_size = self.heap.len();
        while new_heap_size < minimum_required {
            new_heap_size = min(new_heap_size * 2, MAX_MEMORY_SIZE);
        }

        // println!(
        //     "LAKESIS | GC: Expanding heap to {} ({} bytes)",
        //     human_readable_byte_size(new_heap_size as u64),
        //     new_heap_size
        // );

        self.heap.resize(new_heap_size);
        self.regions.extend(new_heap_size);

        // println!("LAKESIS | GC: Done expanding heap");

        match self.regions.allocate(data_size as usize, allocation_id) {
            HeapRegionAllocationResult::Success { base, id } => Ok((base, id)),
            HeapRegionAllocationResult::Error(e) => Err(e),
            HeapRegionAllocationResult::OutOfMemory => {
                self.report_out_of_memory(data_size);
                Err(Error::new("Out of memory"))
            }
        }
    }

    fn report_out_of_memory(&self, data_size: UWord) {
        let total_size = total_region_len(data_size as usize) as UWord;
        println!(
            "LAKESIS | Out of memory - Requested: Data {} ({} bytes) / Total {} ({} bytes)",
            human_readable_byte_size(data_size),
            data_size,
            human_readable_byte_size(total_size),
            total_size
        );
        println!("{}", self);
    }

    fn deallocate(&mut self, id: AllocationId) -> VoidResult {
        let allocation = self
            .allocations
            .remove(id)
            .ok_or_else(|| Error::new("Invalid allocation ID"))?;

        self.virtual_mapper.unmap(allocation.virtual_block)?;
        self.regions.deallocate(allocation.region)?;

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

        if offset >= allocation.data_length {
            return Err(Error::new("Tried to access unmapped memory"));
        }

        Ok((allocation, offset))
    }

    fn addr_to_indices(&self, addr: UWord, size: UWord) -> Result<(usize, usize)> {
        let (allocation, offset) = self.addr_to_allocation(addr)?;

        let readable_len = allocation.data_length - offset;
        if readable_len < size as usize {
            return Err(Error::new(&format!(
                "Tried to access {} bytes but only {} are available",
                size, readable_len
            )));
        }

        let start = allocation.start + offset;
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

        Ok((
            allocation.bitfield_start(),
            allocation.bitfield_end(),
            word_offset,
        ))
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

    fn get_bitfield(&self, allocation: &Allocation) -> &BitSlice<Lsb0, u8> {
        let start = allocation.bitfield_start();
        let end = allocation.bitfield_end();

        let slice = &self.heap[start..end];
        slice.view_bits()
    }
}

impl Display for Memory {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Allocations:")?;

        let mut sorted_allocations: Vec<&Allocation> = self.allocations.iter().collect();
        sorted_allocations.sort_unstable_by_key(|x| x.start);
        for allocation in sorted_allocations {
            write!(f, "\n  {}", allocation)?;
        }

        write!(f, "\n{}", self.virtual_mapper)?;
        write!(f, "\n{}", self.regions)?;

        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Allocation {
    id: AllocationId,
    start: usize,
    data_length: usize,
    is_collectible: bool,
    name: Option<String>,
    virtual_block: VirtualAddressBlockId,
    region: HeapRegionId,
}

impl Allocation {
    fn data_end(&self) -> usize {
        self.start + self.data_length
    }

    fn bitfield_start(&self) -> usize {
        self.data_end()
    }

    fn bitfield_len(&self) -> usize {
        bitfield_len(self.data_length)
    }

    fn bitfield_end(&self) -> usize {
        self.bitfield_start() + self.bitfield_len()
    }

    fn end(&self) -> usize {
        self.bitfield_end()
    }

    fn length(&self) -> usize {
        self.end() - self.start
    }
}

impl Display for Allocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {:08X} {:>10} {} {} {}",
            self.id,
            if self.is_collectible { " " } else { "!" },
            self.start,
            human_readable_byte_size(self.data_length as u64),
            self.region,
            self.virtual_block,
            match &self.name {
                None => "",
                Some(s) => s,
            }
        )
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
            id: Default::default(),
            base: base_addr,
            size,
            allocation,
        });

        for (page, addr) in Self::pages_of(base_addr, size) {
            self.mappings.insert(
                addr,
                VirtualAddressMapping {
                    block: block_id,
                    offset: page as usize * VIRTUAL_PAGE_SIZE as usize,
                },
            );
            self.next_address += VIRTUAL_PAGE_SIZE;
        }

        Ok((base_addr, block_id))
    }

    fn unmap(&mut self, id: VirtualAddressBlockId) -> VoidResult {
        let block = self
            .blocks
            .remove(id)
            .ok_or_else(|| Error::new("Invalid virtual block ID"))?;

        for (_, addr) in Self::pages_of(block.base, block.size) {
            self.mappings.remove(&addr);
        }

        Ok(())
    }

    fn get(&self, id: VirtualAddressBlockId) -> Option<&VirtualAddressBlock> {
        self.blocks.get(id)
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

    fn pages_of(base_addr: UWord, size: UWord) -> Vec<(usize, UWord)> {
        let mut pages = Vec::with_capacity(size as usize / VIRTUAL_PAGE_SIZE as usize);

        for page in 0..=(size / VIRTUAL_PAGE_SIZE) {
            pages.push((page as usize, base_addr + page * VIRTUAL_PAGE_SIZE));
        }

        pages
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
            "{} {:08X} - {:08X} {:>10} {}",
            self.id,
            self.base,
            self.end(),
            human_readable_byte_size(self.size),
            self.allocation
        )
    }
}

#[derive(Clone, Debug)]
struct HeapRegions {
    map: IdHashMap<HeapRegion>,
    in_order: Vec<HeapRegionId>,
}

enum HeapRegionAllocationResult {
    Success { base: usize, id: HeapRegionId },
    OutOfMemory,
    Error(Error),
}

impl HeapRegions {
    fn new(size: usize) -> HeapRegions {
        let mut regions = HeapRegions {
            map: IdHashMap::new(),
            in_order: Vec::with_capacity(1),
        };

        let id = regions.map.insert(HeapRegion {
            id: Default::default(),
            state: HeapRegionState::Free,
            base: 0,
            length: size,
        });

        regions.in_order.push(id);

        regions
    }

    fn get(&self, id: HeapRegionId) -> Option<&HeapRegion> {
        self.map.get(id)
    }

    fn used_bytes(&self) -> usize {
        self.map
            .iter()
            .filter(|x| x.is_used())
            .map(|x| x.length)
            .sum()
    }

    fn allocate(
        &mut self,
        data_size: usize,
        allocation: AllocationId,
    ) -> HeapRegionAllocationResult {
        let total_size = total_region_len(data_size as usize);

        let (index, region) = match self
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
        {
            None => return HeapRegionAllocationResult::OutOfMemory,
            Some(x) => x,
        };

        let region_id = region.id;

        if region.length > total_size {
            let new_region = HeapRegion {
                id: Default::default(),
                state: HeapRegionState::Free,
                base: region.base + total_size,
                length: region.length - total_size,
            };
            let new_region_id = self.map.insert(new_region);
            self.in_order.insert(index + 1, new_region_id);
        }

        let region = self.map.get_mut(region_id).unwrap();
        region.length = total_size;
        region.state = HeapRegionState::Used(allocation);

        HeapRegionAllocationResult::Success {
            base: region.base,
            id: region.id,
        }
    }

    fn deallocate(&mut self, id: HeapRegionId) -> VoidResult {
        let region = self
            .map
            .get(id)
            .ok_or_else(|| Error::new("Invalid region ID"))?;

        let index = self
            .in_order
            .iter()
            .position(|&x| x == region.id)
            .expect("Heap region wasn't in the order array");

        let has_left = index > 0;
        let has_right = index < self.in_order.len() - 1;

        // Empty Free Empty
        if !has_left && !has_right {
            return self.deallocate_island(id);
        }

        if !has_left && has_right {
            let right_id = *self.in_order.get(index + 1).unwrap();
            let right = self.map.get(right_id).unwrap();

            // Empty Free Used
            if right.is_used() {
                return self.deallocate_island(id);
            }

            // Empty Free Free
            return self.join_free_right(index);
        }

        if has_left && !has_right {
            let left_id = *self.in_order.get(index - 1).unwrap();
            let left = self.map.get(left_id).unwrap();

            // Used Free Empty
            if left.is_used() {
                return self.deallocate_island(id);
            }

            // Free Free Empty
            return self.join_free_right(index - 1);
        }

        let right_id = *self.in_order.get(index + 1).unwrap();
        let left_id = *self.in_order.get(index - 1).unwrap();

        let right = self.map.get(right_id).unwrap();
        let left = self.map.get(left_id).unwrap();

        // Used Free Used
        if left.is_used() && right.is_used() {
            return self.deallocate_island(id);
        }

        // Used Free Free
        if left.is_used() && right.is_free() {
            return self.join_free_right(index);
        }

        // Free Free Used
        if left.is_free() && right.is_used() {
            return self.join_free_right(index - 1);
        }

        // Free Free Free
        self.join_free_right(index - 1)?;
        self.join_free_right(index - 1)?;
        Ok(())
    }

    fn compact(&mut self, heap: &mut [u8]) {
        let end = self.map.get(*self.in_order.last().unwrap()).unwrap().end();

        let mut index = 0;
        let mut next_base = 0;
        while index < self.in_order.len() {
            let region_id = self.in_order[index];
            let region = self.map.get(region_id).unwrap();

            if region.is_free() {
                self.in_order.remove(index);
                self.map.remove(region_id);
                // Since this region was removed, the next region will have the same index
                continue;
            }

            heap.copy_within(region.range(), next_base);

            let region = self.map.get_mut(region_id).unwrap();
            region.base = next_base;
            next_base = region.end();

            index += 1;
        }

        if next_base < end {
            let new_region_id = self.map.insert(HeapRegion {
                id: Default::default(),
                state: HeapRegionState::Free,
                base: next_base,
                length: end - next_base,
            });
            self.in_order.push(new_region_id);
        }
    }

    fn extend(&mut self, new_size: usize) {
        let last_region_id = *self.in_order.last().unwrap();
        let last_region = self.map.get(last_region_id).unwrap();

        assert!(new_size >= last_region.end());

        if last_region.is_free() {
            let last_region = self.map.get_mut(last_region_id).unwrap();
            last_region.length = new_size - last_region.base;
            return;
        }

        let new_region = HeapRegion {
            id: Default::default(),
            state: HeapRegionState::Free,
            base: last_region.end(),
            length: new_size - last_region.end(),
        };
        let new_region_id = self.map.insert(new_region);
        self.in_order.push(new_region_id);
    }

    fn join_free_right(&mut self, left_index: usize) -> VoidResult {
        assert!(left_index < self.in_order.len() - 1);

        let right_index = left_index + 1;

        let right_id = self.in_order.remove(right_index);
        let left_id = self.in_order[left_index];

        let right = self.map.remove(right_id).unwrap();
        let left = self.map.get_mut(left_id).unwrap();

        left.state = HeapRegionState::Free;
        left.length += right.length;

        Ok(())
    }

    fn deallocate_island(&mut self, id: HeapRegionId) -> VoidResult {
        let region = self
            .map
            .get_mut(id)
            .ok_or_else(|| Error::new("Invalid heap region ID"))?;

        region.state = HeapRegionState::Free;

        Ok(())
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

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
struct HeapRegion {
    id: HeapRegionId,
    state: HeapRegionState,
    base: usize,
    length: usize,
}

impl HeapRegion {
    fn is_free(&self) -> bool {
        self.state.is_free()
    }

    fn is_used(&self) -> bool {
        self.state.is_used()
    }

    fn end(&self) -> usize {
        self.base + self.length
    }

    fn range(&self) -> Range<usize> {
        self.base..self.end()
    }
}

impl Display for HeapRegion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {:08X} {:>10} {}",
            self.id,
            self.base,
            human_readable_byte_size(self.length as u64),
            self.state
        )
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
enum HeapRegionState {
    Free,
    Used(AllocationId),
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

impl Display for HeapRegionState {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            HeapRegionState::Free => write!(f, "Free"),
            HeapRegionState::Used(id) => write!(f, "{}", *id),
        }
    }
}

#[derive(Debug)]
struct Heap {
    ptr: *mut u8,
    len: usize,
}

impl Heap {
    fn new(size: usize) -> Heap {
        let mut heap = Heap {
            ptr: unsafe { ptr::null_mut() },
            len: size,
        };

        heap.ptr = unsafe { alloc::alloc(heap.layout()) };

        heap
    }

    fn layout(&self) -> Layout {
        Layout::array::<u8>(self.len).unwrap()
    }

    fn resize(&mut self, new_size: usize) {
        self.ptr = unsafe { alloc::realloc(self.ptr, self.layout(), new_size) };
        self.len = new_size;
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        unsafe {
            alloc::dealloc(self.ptr, self.layout());
        };

        self.ptr = ptr::null_mut();
        self.len = 0;
    }
}

impl Index<Range<usize>> for Heap {
    type Output = [u8];

    fn index(&self, index: Range<usize>) -> &Self::Output {
        unsafe { slice::from_raw_parts(self.ptr.add(index.start), index.len()) }
    }
}

impl IndexMut<Range<usize>> for Heap {
    fn index_mut(&mut self, index: Range<usize>) -> &mut Self::Output {
        unsafe { slice::from_raw_parts_mut(self.ptr.add(index.start), index.len()) }
    }
}

impl Deref for Heap {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl DerefMut for Heap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

trait IdWrapper: Hash + Eq + Copy + Default + Deref<Target = u64> + DerefMut {
    fn new(x: u64) -> Self;

    fn first() -> Self {
        Self::new(1)
    }

    fn next(&self) -> Self {
        Self::new(**self + 1)
    }
}

trait StructWithId {
    type Id: IdWrapper;
    fn get_id(&self) -> Self::Id;
    fn set_id(&mut self, id: Self::Id);
}

macro_rules! entity_id {
    ( $entity:ident , $id_wrapper:ident , $prefix:expr ) => {
        #[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Default, Debug, Hash)]
        struct $id_wrapper(u64);

        impl IdWrapper for $id_wrapper {
            fn new(x: u64) -> Self {
                Self(x)
            }
        }

        impl StructWithId for $entity {
            type Id = $id_wrapper;

            fn get_id(&self) -> Self::Id {
                self.id
            }
            fn set_id(&mut self, id: Self::Id) {
                self.id = id
            }
        }

        impl Display for $id_wrapper {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}{:04}", $prefix, **self)
            }
        }

        impl Deref for $id_wrapper {
            type Target = u64;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl DerefMut for $id_wrapper {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
    };
}

entity_id!(Allocation, AllocationId, "A");
entity_id!(VirtualAddressBlock, VirtualAddressBlockId, "V");
entity_id!(HeapRegion, HeapRegionId, "R");

#[derive(Clone, Debug)]
struct IdHashMap<T>
where
    T: StructWithId,
{
    next_id: T::Id,
    map: HashMap<T::Id, T>,
}

impl<T> IdHashMap<T>
where
    T: StructWithId,
{
    fn new() -> IdHashMap<T> {
        IdHashMap {
            next_id: T::Id::first(),
            map: HashMap::new(),
        }
    }

    fn peek_next_id(&self) -> T::Id {
        self.next_id
    }

    fn insert(&mut self, mut item: T) -> T::Id {
        let id = self.next_id;
        self.next_id = self.next_id.next();

        item.set_id(id);
        self.map.insert(id, item);

        id
    }

    fn get(&self, id: T::Id) -> Option<&T> {
        self.map.get(&id)
    }

    fn get_mut(&mut self, id: T::Id) -> Option<&mut T> {
        self.map.get_mut(&id)
    }

    fn remove(&mut self, id: T::Id) -> Option<T> {
        self.map.remove(&id)
    }

    fn iter(&self) -> std::collections::hash_map::Values<T::Id, T> {
        self.map.values()
    }

    fn iter_mut(&mut self) -> std::collections::hash_map::ValuesMut<T::Id, T> {
        self.map.values_mut()
    }

    fn id_iter(&self) -> std::collections::hash_map::Keys<T::Id, T> {
        self.map.keys()
    }

    fn entry_iter(&self) -> std::collections::hash_map::Iter<T::Id, T> {
        self.map.iter()
    }

    fn len(&self) -> usize {
        self.map.len()
    }
}

impl<'a, T> IntoIterator for &'a IdHashMap<T>
where
    T: StructWithId,
{
    type Item = &'a T;
    type IntoIter = std::collections::hash_map::Values<'a, T::Id, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

fn bitfield_len(data_len: usize) -> usize {
    divide_round_up(data_len, WORD_BYTE_SIZE as usize * 8)
}

fn total_region_len(data_len: usize) -> usize {
    data_len + bitfield_len(data_len)
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
    divide_round_up(value, alignment) * alignment
}

fn divide_round_up<T>(dividend: T, divisor: T) -> T
where
    T: Copy + Add<Output = T> + Sub<Output = T> + Div<Output = T> + From<u8>,
{
    (dividend + divisor - 1.into()) / divisor
}

fn human_readable_byte_size(value: impl Into<u64>) -> String {
    ByteSize(value.into()).to_string_as(true)
}
