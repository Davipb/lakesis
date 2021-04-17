;; Linked List program
; Inteded to provide a 'closer to real-world' use case
; Creates a linked list of numbers that is always in ascending order
; Continuously generates random numbers and inserts them into the list or removes them if they're already there
; If there are no more items in the list, halts the program

.define RAND_MASK 0b111

jmp main

; Linked list memory layout:
; Size: 3w
; +0w data
; +1w [R]previous
; +2w [R]next

; Params: data
LinkedList::new_node:
    new 3w, r0
    mov [sp+2w], [r0]
    mov 0, [r0+1w]
    mov 0, [r0+2w]
    ret

; Params: [R]linked_node, [R]new_node
LinkedList::insert_before:
    mov [sp+3w], r0 ; r0 = new_node
    mov [sp+2w], r1 ; r1 = linked_node
    mov [r1+1w], r2  ; r2 = linked_node.previous = previous_node

    push [r1]
    push [r0]
    push LinkedList::insert_before::str
    ref [sp+1w]
    push LinkedList::insert_before::str_len
    native 0
    pop r3
    pop r3
    pop r3
    pop r3

    mov [r1+1w], [r0+1w] ; new_node.previous = linked_node.previous
    mov r1, [r0+2w] ; new_node.next = linked_node
    mov r0, [r1+1w] ; linked_node.previous = new_node

    cmp r2, 0
    jeq LinkedList::insert_before::end
    mov r0, [r2+2w] ; previous_node.next = new_node

    LinkedList::insert_before::end:
    ret

    LinkedList::insert_before::str: .string LinkedList::insert_before::str_len "Inserting %u before %u\n"

; Params: [R]linked_node, [R]new_node
LinkedList::insert_after:
    mov [sp+3w], r0 ; r0 = new_node
    mov [sp+2w], r1 ; r1 = linked_node
    mov [r1+2w], r2 ; r2 = linked_node.next = next_node

    push [r1]
    push [r0]
    push LinkedList::insert_after::str
    ref [sp+1w]
    push LinkedList::insert_after::str_len
    native 0
    pop r3
    pop r3
    pop r3
    pop r3

    mov [r1+2w], [r0+2w] ; new_node.next = linked_node.next
    mov r1, [r0+1w] ; new_node.previous = linked_node
    mov r0, [r1+2w] ; linked_node.next = new_node

    cmp r2, 0
    jeq LinkedList::insert_after::end
    mov r0, [r2+1w] ; next_node.previous = new_node

    LinkedList::insert_after::end:
    ret

    LinkedList::insert_after::str: .string LinkedList::insert_after::str_len "Inserting %u after %u\n"

; Params: [R]node
LinkedList::remove:
    mov [sp+2w], r0 ; r0 = node

    push [r0]
    push LinkedList::remove::str
    ref [sp+1w]
    push LinkedList::remove::str_len
    native 0
    pop r3
    pop r3
    pop r3

    ; Fix previous if non-null
    mov [r0+1w], r1 ; r1 = previous
    cmp r1, 0
    jeq LinkedList::remove::fix_next

    mov [r0+2w], [r1+2w] ; previous.next = node.next

    LinkedList::remove::fix_next:
    ; Fix next if non-null
    mov [r0+2w], r1 ; r1 = next
    cmp r1, 0
    jeq LinkedList::remove::end

    mov [r0+1w], [r1+1w] ; next.previous = node.previous

    LinkedList::remove::end:
    ret

    LinkedList::remove::str: .string LinkedList::remove::str_len "Removing %u\n"

; Params: [R]node
LinkedList::length:
    mov [sp+2w], r0
    mov 0, r1

    LinkedList::length::loop:
    cmp r0, 0
    jeq LinkedList::length::end
    add 1, r1
    mov [r0+2w], r0
    jmp LinkedList::length::loop

    LinkedList::length::end:
    ret

; Params: [R]node
LinkedList::print:
    mov [sp+2w], r0

    LinkedList::print::loop:
    cmp r0, 0
    jeq LinkedList::print::end
    ; debugmem r0, 3w

    mov LinkedList::print::str_node, r1
    ref r1
    push [r0]
    push r1
    push LinkedList::print::str_node_len
    native 0
    pop r1
    pop r1
    pop r1

    mov [r0+2w], r0
    jmp LinkedList::print::loop
    LinkedList::print::end:
    mov LinkedList::print::str_end, r1
    ref r1
    push r1
    push LinkedList::print::str_end_len
    native 0
    pop r1
    pop r1
    ret

    LinkedList::print::str_node: .string LinkedList::print::str_node_len "%u "
    LinkedList::print::str_end: .string LinkedList::print::str_end_len "\n"

main:
    native 1
    mov r0, r3
    and RAND_MASK, r3
    push r3
    call LinkedList::new_node
    pop r3
    mov r0, r3

    main::loop:
    push r3
    call LinkedList::print
    pop r3

    push 1_000
    native 2
    pop r0

    native 1
    and RAND_MASK, r0
    push r3
    push r0
    call LinkedList::new_node
    pop r3
    pop r3

    ; r0 = new_node
    ; r3 = list_head
    mov r3, r2
    main::loop::find_to_insert:
    cmp [r2], [r0]
    jeq main::loop::remove
    jgt main::loop::insert_middle
    cmp [r2+2w], 0
    jeq main::loop::insert_end
    mov [r2+2w], r2
    jmp main::loop::find_to_insert

    main::loop::remove:
    push r3
    push r2
    call LinkedList::remove
    pop r2
    pop r3
    cmp r2, r3
    jne main::loop::end
    mov [r3+2w], r3
    cmp r3, 0
    jne main::loop::end
    halt

    main::loop::insert_middle:
    push r3
    push r0
    push r2
    call LinkedList::insert_before
    pop r2
    pop r0
    pop r3
    cmp [r3+1w], 0
    jeq main::loop::end
    mov [r3+1w], r3
    jmp main::loop::end

    main::loop::insert_end:
    push r3
    push r0
    push r2
    call LinkedList::insert_after
    pop r2
    pop r0
    pop r3
    jmp main::loop::end

    main::loop::end:
    jmp main::loop
