
;; LinkedList
; long value
; void* next
; void* prev

LinkedList::new: ; value
    new r0, 24  ; output, size in bytes
    mov [sp+8], [r0] ; mov src, target
    mov 0, [r0+8]
    mov 0, [r0+16]
    ret ; return value is in r0


LinkedList::append: ; [r]list, value
    mov [sp+8], r1
LinkedList::append_loop::start:
    mov [r1+8], r0
    cmp r0, 0
    jeq LinkedList::append::loop_end
    mov r0, r1
    jmp LinkedList::append::loop_start
LinkedList::append::loop_end:
    push [sp+16]
    call LinkedList::new
    pop
    mov r0, [r1+8]
    mov r1, [r0+16]
    ret


LinkedList::find: ; [r]list, value
    mov [sp+8], r0
LinkedList::find::loop_start:
    cmp [r0], [sp+16]
    jeq LinkedList::find::loop_end
    mov [r0+8], r0
    jeq LinkedList::find::loop_end
    jmp LinkedList::find::loop_start
LinkedList::find::loop_end:
    ret


LinkedList::remove ; [r]node
    mov [sp+8], r0
    mov [r0+8], r1  ; next
    mov [r0+16], r2 ; previous
    cmp r1, 0
    jeq LinkedList::remove::fix_next_end
    mov r2, [r1+16]
LinkedList::remove::fix_next_end:
    cmp r2, 0
    jeq LinkedList::remove::fix_previous_end
    mov r1, [r2+8]
LinkedList::remove::fix_previous_end:
    ret


;; Vector
; long capacity
; long length
; void* data

Vector::new:
    new r0, 16
    mov 0, [r0]
    mov 0, [r0+8]
    ret

Vector::resize: ; [r]vector, new_capacity
    mov [sp+8], r0  ; get vector ref
    mov [sp+16], r1 ; get new capacity
    mov [r0], r2    ; get old capacity
    cmp r1, r2      ; do nothing if old capacity = new capacity
    jeq Vector::resize::end
    mov r1, [r0]    ; update capacity
    mov [r0+16], r3 ; get old data ref
    new r2, r1      ; make new data ref
    mov r2, [r0+16] ; update vector to use new data
    mov [r0+8], r1
Vector::resize::copy_loop_start:    
    ; r0: vector ref
    ; r1: data index
    ; r2: new data ref
    ; r3: old data ref
    sub 1, r1
    mov [r3], [r2]
    add 8, r3
    add 8, r2
    cmp 0, r1
    jne Vector::resize::copy_loop_start    
Vector::resize::end:
    ret

Vector::append: ; [r]vector, value
    mov [sp+8], r0
    mov [r0], r1
    mov [r0+8], r2
    add 1, r2
    cmp r1, r2
    jge Vector::append::do_append
    mul 2, r1
    push r2 ; not a parameter, but we save it in case it gets clobbered
    push r1
    push r0
    call Vector::resize
    pop r0
    pop r1
    pop r2
Vector::append::do_append:
    mov r2, [r0+8] ; update length
    mov [r0+16], r0 ; get reference to data
    ; get reference to the location where the new value will be
    sub 1, r2 ; -1 to get index from length
    mul 8, r2 ; * 8 to get data byte offset from index
    add r0, r2
    mov [sp+16], [r2]
    ret


Vector::find: ; [r]vector, value
    mov [sp+8], r2
    mov [sp+16], r1
    mov [r2+8], r3
    mov [r2+16], r0
    mov 0, r0
Vector::find::loop:
    cmp [r2], r1
    jeq Vector::find::end
    add 8, r2
    add 1, r0
    cmp r0, r3
    jne Vector::find::loop
    mov 0xFFFFFFFFFFFFFFFF, 
Vector::find::end:
    ret


Vector::remove: ; [r]vector, index
    mov [sp+8], r0  ; vector ref
    mov [r0+16], r1 ; data ref
    mov [r0+8], r2  ; vector len
    sub 1, r2
    mov r2, [r0+8]
    sub 1, r2       ; max index
    mov [sp+16], r3 ; removal index
    mul 8, r3
    add r3, r1
    mov [sp+16], r3    
Vector::remove::loop:
    mov [r1+8], [r1]
    add 8, r1
    add 1, r3
    cmp r3, r2
    jne Vector::remove::loop
    ret

