;; Memory allocation loop

big_loop:
    native 1
    and 0xFFF_FFFF, r0
    new r0, r3

    mov 10, r1
    small_loop:
        native 1
        and 0xFF_FFFF, r0
        new r0, r2
        native 1
        and 0b1, r0
        cmp r0, 0
        jeq small_loop::end
        push r2

        small_loop::end:
        sub 1, r1
        jne small_loop

    ;debugmem
    jmp big_loop
