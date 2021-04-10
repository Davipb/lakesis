;; Kitchen Sink
; This file uses all available instructions

; Simple stuff
nop
mov r0, r0
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

; Memory
new 24, r0
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

halt

subroutine: ret