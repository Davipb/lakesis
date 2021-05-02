# Lakesis
⚠ Work in Progress ⚠

A programming language runtime following in the footsteps of the Java Virtual Machine with its own bytecode, 
dynamic memory allocation, and garbage collection.
This is a personal learning experiment and creating a production-ready language is not its goal.

Check out some examples of what the runtime can already do at the [/asm directory](/asm)!

## Roadmap
- [x] Bytecode encoding and decoding
- [x] Assembler to write bytecode in a more human-friendly way
- [x] Bytecode interpreter
- [x] Simple memory allocation (without GC)
- [X] Virtual/physical address translation
- [x] Simple mark/sweep garbage collection
- [x] Mark/sweep/compact garbage collection

## Usage
The documentation below uses `cargo run` to compile and execute the runtime directly from cargo,
but it works the same if you're using a compiled version 
(just replace `cargon run` with the name of the executable)

* `cargo run help`  
  Prints this usage help
  
* `cargo run asm <source> [output]`  
  Compiles an assembly source code file to an executable
  * `source`: Path of the file containing the assembly source code
  * `output`: Path of the file where the executable will be written to. If not specified, uses the same file as 'source' but with a .bin extension

* `cargo run view <file>`  
  Disassembles an executable and displays its code
  * `file`: Path of the file to disassemble
  
* `cargo run run <file>`    
  Runs a compiled executable
  * `file`: Path of the executable to run
  
* `cargo run runasm <file>`  
  Compiles an assembly source file and immediately runs it
  * `file`: Path of the assembly source code to compile and run


## Work in progress warning
As Lakesis is a work in progress, the sections below may not reflect the current state of the project.
Consider them a living architectural document rather than documentation.

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
Arithmetic and bitwise operations such as ADD and AND between two references or a reference and regular data result in a reference.
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

In the list below, the first line contains the assembly mnemonic of the instruction,
the second line contains the value of the instruction in hexadecimal after masking out the top two bits,
and the third line contains a description of the instruction.
All values following the mnemonic or non-numeric bytes in the opcode are operands 
(check the section below for an overview of all available operands and addressing modes).

#### Special
* NOP  
`00`  
Does nothing
* NATIVE num  
`1C num`  
Calls the native function identified by `num`. For a list of available native functions, check the section below.
* DEBUGMEM
`3D`  
Dumps the current state of memory to the console for debugging purposes
* DEBUGDUMP addr len  
`3D addr len`  
Dumps `len` bytes of memory starting at `addr` to the console for debugging purposes
* DEBUGCPU num  
`3E num`  
Dumps the entire state of the CPU to the console along with an arbitrary number for debugging purposes
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
    * CF = operation caused an underflow
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
    * CF = 0
* SHR bits, x  
`0B bits x`  
Shifts the value of `x` by `bits` bits to the right and stores the result in `x`. The current data type of `x` is maintained.
    * ZF = result is zero
    * CF = 0

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
Jumps to the specified address if ZF = 1 or CF = 0 (a < b)
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

A sign bit/magnitude model was chosen instead of the usual two's complement to reduce the space taken by common negative
values such as -1. 
While this means that negative zero is technically allowed in operands, the assembler will never generate a negative
zero and the interpreter will decode it as a regular zero.

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

## Native functions
Native functions can be called through the NATIVE `1C` instruction. The native functions are:

* `00`  
Print  
Accepts a 64-bit length `L`, a reference `R`, and a variable number of extra arguments.
Reads `L` bytes from the memory region `R` points to and interprets them as an ASCII format string,
using the extra arguments to fill in formatter placeholders found in the text  before printing the string to stdout. 
The placeholders are:
  * `%u` Unsigned integer
  * `%d` Signed integer
  * `%s` UTF-8 string. Must supply two arguments: string length and string reference, in that order
  * `%%` Literal percent-sign character

* `01`  
Random  
Generates a random number between 0 and 0xFFFFFFFFFFFFFFFF and stores it in R0.

* `02`  
Sleep  
Takes a 64-bit number of milliseconds as an argument and sleeps for that amount.

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
