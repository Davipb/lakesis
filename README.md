# Lakesis
A garbage-collected runtime running in a minimal but flexible VM architecture (à la JVM and CLR).
This is a personal learning experiment and creating a full finished product is not its goal. Do not use this in production.

## Architecture
* Byte-addressable with 64-bit (8 byte) words
* Four 1-word registers, R0-R3
* Special-purpose stack pointer (SP) register
    * Can be read from freely
    * Can only be written to with PUSH or POP
* Two flags: Zero (ZF) and Carry (CF)
* Tracing garbage collection

## Memory Management
Every register and memory location has a *data type*, which can be "reference" or "regular data".
Registers and memory locations are said to be *marked as a reference* or *marked as regular data*.

At boot, every register and memory location is marked as regular data by default.
When a new memory region is allocated with NEW, the register or memory location that receives its address is marked as a reference.
Arithmetical and bitwise operations such as ADD and AND between two references or a reference and regular data result in a reference.
Copy instructions such as MOV, PUSH, and POP preserve the data type of the source.

In case this automatic tracking fails, the REF and UNREF instructions can be used to manually mark a register or memory location as containing a reference or data.

Garbage collection is done by locating all references on the stack and in registers, finding the memory regions they point to, locating all references in those memory regions, and so on recursively until all reachable memory regions are found. This is called *tracing*. The unreachable memory regions can then be freed, and existing regions compacted in memory (an indirection table is used to allow for physical addressed to change independently of addresses used by code).

Garbage collection is usually done when NEW is called and there isn't enough contiguous space left on the heap to allocate the specified number of bytes. Alternatively, the GC instruction can be used to force a garbage collection cycle at will.


## ISA
Opcodes are composed of two parts: The *instruction* and its *operands*.

### Instructions
Instructions are encoded as a single byte.
The two most significant bits of the byte indicate how many operands that instruction expects, from 0 `00` to 2 `10` -- the value 3 `11` is reserved and shouldn't be used.
The remaining lower bits identify the instruction itself.

First line contains the assembly mnemonic of the instruction,
second line contains the value of the instruction in hexadecimal after masking out the top two bits,
third line contains a description of the instruction.
All values following the mnemonic or non-numeric bytes in the opcode are operands (view section below on all available operands).

#### Special
* NOP  
`00`  
Does nothing
* HALT  
`3F`  
Stops program execution

#### Arithmetic
* ADD src, dst  
`02 src dst`  
Adds `src` to `dst` and stores the result in `dst`. If either operand is a reference, `dst` is marked as reference. Otherwise, it is marked as regular data.
    * ZF = result is zero
    * CF = operation caused an overflow
* SUB src, dst  
`03 src dst`  
Subtracts `src` from `dst` and stores the result in `dst`. If either operand is a reference, `dst` is marked as reference. Otherwise, it is marked as regular data.
    * ZF = result is zero
    * CF = operation *didn't* cause an underflow
* MUL src, dst  
`04 src dst`  
Multiplies `src` and `dst` and stores the result in `dst`. If either operand is a reference, `dst` is marked as reference. Otherwise, it is marked as regular data.
* DIV src, dst  
`05 src dst`  
Divides `dst` by `src` and stores the result in `dst`. If either operand is a reference, `dst` is marked as reference. Otherwise, it is marked as regular data.

#### Bitwise
* AND src, dst  
`06 src dst`  
Performs a bitwise AND between `dst` and `src` and stores the result in `dst`. If either operand is a reference, `dst` is marked as reference. Otherwise, it is marked as regular data.
    * ZF = result is zero
    * CF = 0
* OR src, dst  
`07 src dst`  
Performs a bitwise OR between `dst` and `src` and stores the result in `dst`. If either operand is a reference, `dst` is marked as reference. Otherwise, it is marked as regular data.
    * ZF = result is zero
    * CF = 0
* XOR src, dst  
`08 src dst`  
Performs a bitwise XOR between `dst` and `src` and stores the result in `dst`. If either operand is a reference, `dst` is marked as reference. Otherwise, it is marked as regular data.
    * ZF = result is zero
    * CF = 0
* NOT x  
`09 x`  
Negates all bits of `x`. The current data type of `x` is maintained.
    * ZF = result is zero
    * CF = 0
* SHL bits, x  
`0A bits x`  
Shifts the value of `x` by `bits` bits to the left and stores the result in `x`. The current data type of `x` is maintained.
    * ZF = result is zero
    * CF = at least one bit shifted out of the number was set
* SHR bits, x  
`0B bits x`  
Shifts the value of `x` by `bits` bits to the right and stores the result in `x`. The current data type of `x` is maintained.
    * ZF = result is zero
    * CF = at least one bit shifted out of the number was set

#### Flow control
* CMP a, b  
`0C a b`  
Compares the values of a and b
    * ZF = if a is equal to b
    * CF = if a is greater than or equal to b
* JMP addr  
`0D addr`  
Jumps to the specified address
* JEQ addr  
`0E addr`  
Jumps to the specified address if ZF = 1 (a == b)
* JNE addr  
`0F addr`  
Jumps to the specified address if ZF = 0 (a != b)
* JGT addr  
`10 addr`  
Jumps to the specified address if ZF = 0 and CF = 1 (a > b)
* JGE addr  
`11 addr`  
Jumps to the specified address if CF = 1 (a >= b)
* JLT addr  
`12 addr`  
Jumps to the specified address if CF = 0 (a < b)
* JLE addr  
`13 addr`  
Jumps to the specified address if ZF = 1 or CF = 0 (a <= b)
* CALL addr  
`14 addr`  
Pushes the address of the next instruction to the stack and jumps to the specified address. The pushed address is marked as a reference. Used to call subroutines.
* RET  
`15`  
Pops an address from the stack and jumps to it. Used to return from subroutines.

#### Memory
* MOV src, dst  
`01 src dst`  
Copies `src` to `dst` without any changes. `dst` inherits the data type of `src`.
* PUSH x  
`16 x`  
Pushes `x` to the stack and decrements SP by 8. The memory location at the stack where `x` was pushed to inherits the data type of `x`.
* POP x  
`17 x`  
Pops the item at the top of the stack to `x` and increments SP by 8. `x` inherits the data type of the popped item.
* NEW size, dst  
`18 size dst`  
Allocates a new memory region of size `size`, puts its address in `dst`, and marks `dst` as a reference.
* GC  
`19`  
Forces the garbage collector to run fully
* REF x  
`1A x`  
Marks `x` as containing a reference. Use with extreme caution -- in general, the automatic reference tracker should take care of this for you.
* UNREF x  
`1B x`  
Marks `x` as containing regular data. Use with extreme caution -- in general, the automatic reference tracker should take care of this for you.


### Operands
Operands always start with a single byte whose bits follow the pattern:  
`aarr snnn`  
Where:
* `a` = Addressing mode identifier
* `r` = Register number
* `s` = Sign of the operand value; 0 = positive, 1 = negative
* `n` = Number of bytes used by the operand value 
* The operand value `v` is encoded as little-endian in the `n` bytes that follow the operand, using the sign `s`.

The available addressing modes are:
* `00`  
n / -n  
Immediate
    * When used as a source, the `v` is used directly
    * Cannot be used as a destination
    * Assembly syntax example:  
    `PUSH 1234`
* `01`  
Rn  
Register  
    * When used as a source, the value stored in register `r` is used
    * When used as a destination, values are written to register `r`
    * Assembly syntax example:  
    `PUSH R0`
* `10`  
[Rn+x] / [Rn-x]  
Reference  
    * Reads the value stored in register `r`, adds `v` to it, and interprets that as a memory address
    * When used as a source, the value stored in the calculated memory address is used
    * When used as a destination, values are written to the calculated memory address
    * Assembly syntax examples:  
    `PUSH [R0]`  
    `PUSH [R0+8]`  
    `PUSH [R0-8]`
* `11`  
[SP+x]  
Stack reference
    * Adds `v` to the stack pointer, and interprets that as a memory address. Negative `v` values are not allowed.
    * When used as a source, the value stored in the calculated memory address is used
    * When used as a destination, values are written to the calculated memory address
    * Assembly syntax examples:  
    `PUSH [SP]`  
    `PUSH [SP+8]`

## Calling convention
Arguments are pushed to the stack in reverse order and cleaned up by the caller. 
Values are returned in R0.
All registers and flags are caller-saved.

## Executable file format
Executables are loaded into memory as-is and start executing from their very first byte.
The executable is always loaded at address 0, so jump instructions can use absolute offsets from the file itself as addresses.

## Naming
[Lachesis](https://en.wikipedia.org/wiki/Lachesis), literally "alotter", was the greek goddess who "measured the thread of life", deciding how long a person should live.
Being an experiment in garbage collection first and foremost -- the "life" of memory, so to speak -- a variation of this name was chosen for the project.
