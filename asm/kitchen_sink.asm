;; Kitchen Sink
; This file uses all available instructions and assembler features

; .define: Simple compile-time constant definition. Can only be used to define numbers.
; The defined label can be used anywhere a regular label can be used
.define COMPILE_TIME_CONSTANT 1337

; Simple stuff
nop
mov COMPILE_TIME_CONSTANT, r0
push r0
pop r0
add r0, r0
sub r0, r0
mul r0, r0
div 1, r0
and r0, r0
or r0, r0
xor r0, r0
not r0
shl r0, r0
shr r0, r0
cmp r0, r0

; Number bases and multipliers
; 0x?? = Hexadecimal
; 0b?? = Binary
; ??w = Word-size, value is multiplied by 8
mov 1, r0
mov -1, r0
mov 0xA, r0
mov -0xA, r0
mov 0b10, r0
mov -0b10, r0
mov 1w, r0
mov -1w, r0
mov 0xAw, r0
mov -0xAw, r0
mov 0b10w, r0
mov -0b10w, r0

; Memory
new 128w, r0
unref r0
ref r0
gc

; Jumps
jmp l1
l1: jeq l2
l2: jne l3
l3: jgt l4
l4: jge l5
l5: jlt l6
l6: jle l7
l7: call subroutine

; Native
mov string, r0
ref r0

push r0
push string_len
native 0
pop r0
pop r0

halt

subroutine: ret

; .align: fills the assembled binary with zeroes until the current address is a multiple of the specified value
.align 1w
; .string: Writes the specified string as UTF-8 directly to the assembled binary at the current address
; If a label is specified before the string, the string's size in bytes is stored in that label.
string: .string string_len "Hello\n\"Beautiful\"\n\\world\\!"
