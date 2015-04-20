use16

org 0x7C00

boot:
    ; initialize segment registers
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    ; initialize stack
    mov sp, 0x7bfe

    mov [drive], dl

    call read_disk

    jmp startup


read_disk:
    mov si, DAPACK      ; address of "disk address packet"
    mov ah, 0x42        ; AL is unused
    mov dl, [drive]     ; DL comes with drive number already
    int 0x13
    jc error
    ret

error:
    mov si, .msg
.loop:
    lodsb
    or al, al
    jz .done
    mov ah, 0x0e
    int 0x10
    jmp .loop
.done:
    cli
    hlt
    .msg db "could not read disk", 0

drive:
    db 0

DAPACK:
	db	0x10
	db	0
blkcnt:	dw	(kernel_file.end - unfs_header)/512		; int 13 resets this to # of blocks actually read/written
db_add:	dw	unfs_header		; memory buffer destination address (0:7c00)
	dw	0		; in memory page zero
d_lba:	dd	(unfs_header - boot)/512	; put the lba to read in this spot
	dd	0		; more storage bytes only for big lba's ( > 4 bytes )

times 510-($-$$) db 0
db 0x55
db 0xaa

unfs_header:
.signature:
    db 'U'
    db 'n'
    db 'F'
    db 'S'
.version:
    dd 1
.root_sector_list:
    dq (unfs_root_sector_list - boot)/512
.free_space_lba:
    dq 0
.name:
    db "Root Filesystem",0

    align 512, db 0
.end:

startup:
	call vesa
	call initialize.fpu
	call initialize.sse
	call initialize.pic

    ; load protected mode GDT and IDT
    cli
    lgdt [gdtr]
    lidt [idtr]
    ; set protected mode bit of cr0
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    ; far jump to load CS with 32 bit segment
    jmp 0x08:protected_mode

%include "asm/vesa.asm"
%include "asm/initialize.asm"

protected_mode:
    use32
    ; load all the other segments with 32 bit data segments
    mov eax, 0x10
    mov ds, eax
    mov es, eax
    mov fs, eax
    mov gs, eax
    mov ss, eax
    ; set up stack
    mov esp, 0x1FFFF0
	
    call mouse.init
    
    ;rust init
	mov eax, [kernel_file + 0x18]
	mov [interrupts.callback], eax
    int 255
    
    ;rust will handle interrupts
    sti
.lp:
	hlt
    jmp .lp

gdtr:
    dw (gdt_end - gdt) + 1  ; size
    dd gdt                  ; offset

gdt:
    ; null entry
    dq 0
    ; code entry
    dw 0xffff       ; limit 0:15
    dw 0x0000       ; base 0:15
    db 0x00         ; base 16:23
    db 0b10011010   ; access byte - code
    db 0x4f         ; flags/(limit 16:19). flag is set to 32 bit protected mode
    db 0x00         ; base 24:31
    ; data entry
    dw 0xffff       ; limit 0:15
    dw 0x0000       ; base 0:15
    db 0x00         ; base 16:23
    db 0b10010010   ; access byte - data
    db 0x4f         ; flags/(limit 16:19). flag is set to 32 bit protected mode
    db 0x00         ; base 24:31
gdt_end:

%include "asm/interrupts.asm"
%include "asm/mouse.asm"

times (0xC000-0x1000)-0x7C00-($-$$) db 0

kernel_file:
incbin "kernel.bin"
align 512, db 0
.end:

%include "unfs.asm"
