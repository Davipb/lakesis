
push 0
call print_num
pop r0

push -1
call print_num
pop r0

push 1
call print_num
pop r0

push 1337
call print_num
pop r0

push str_data_len
push str_data
call print_str
pop r0
pop r0

halt

print_num:
    mov [sp+2w], r0
    push r0
    push r0

    mov fmt_num, r0
    ref r0
    push r0

    push fmt_num_len
    native 0
    pop r0
    pop r0
    pop r0
    pop r0

    ret

print_str:
    mov [sp+2w], r0
    mov [sp+3w], r1
    mov fmt_str, r2
    ref r0
    ref r2

    push r1
    push r0
    push r2
    push fmt_str_len
    native 0
    pop r0
    pop r0
    pop r0
    pop r0

    ret

fmt_num:
.string fmt_num_len "Signed: %d\nUnsigned: %u\n\n"

fmt_str:
.string fmt_str_len "%s\n"

str_data:
.string str_data_len " %s Hello %d World ! \n"
