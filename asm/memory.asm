;; Memory allocation loop

loop:
mov r2, r3
mov r1, r2
mov r0, r1
native 1
and 0xFFF_FFFF, r0
new r0, r0
debugmem
jmp loop
