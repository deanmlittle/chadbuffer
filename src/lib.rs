#![cfg_attr(target_os = "solana", feature(asm_experimental_arch))]

use {
    sbpf_asm_macros::set_return_imm,
    solana_program::{
        log,
        program_memory::{sol_memcmp, sol_memcpy},
    },
    std::mem::size_of,
    std::slice::{from_raw_parts, from_raw_parts_mut},
};

#[allow(non_camel_case_types)]
type u24 = [u8;3];

// Alignment
pub const ALIGNMENT:        usize = 0x0008;
pub const PUBKEY_LENGTH:    usize = 0x0020;

// Bit masks
pub const SIG_MUT_NODUP:    u32 = 0x0101ff;
pub const U24_MASK:         usize = 0xffffff;

// Signer offsets
pub const SIGNER_HEADER:    usize = 0x0008;
pub const SIGNER_KEY:       usize = 0x0010;
pub const SIGNER_LAMPORTS:  usize = 0x0050;

// Buffer offsets
pub const BUFFER_OWNER:     usize = 0x2890;
pub const BUFFER_SIZE:      usize = 0x28b8;
pub const BUFFER_LAMPORTS:  usize = 0x28b0;
pub const BUFFER_AUTH:      usize = 0x28c0;
pub const BUFFER_DATA:      usize = 0x28e0;

// Instruction offsets
pub const IX_MIN_OFFSET:    usize = 0x50c8;

#[no_mangle]
/// # Safety
/// Where we're going, we don't need memory safety.
pub unsafe extern "C" fn entrypoint(input: *mut u8) {
    // 1. Account checks

    // By knowing we have 2 accounts and the signer account is a non-dup,
    // we can skip checking the buffer account, as it will fail mutability anyway.

    // 1a) Check we have 2 accounts and signer is a nodup mut signer
    if *input as u64 != 2 {
        log::sol_log("Wrong number of accounts");
        set_return_imm!(1);
        return;
    }

    // 1b) If we have 2 accounts and signer is non-dup, we can skip checking the buffer
    if *(input.add(SIGNER_HEADER) as *const u32) != SIG_MUT_NODUP {
        log::sol_log("Missing signer");
        set_return_imm!(1);
        return;
    }

    // 2. Get IX data offset, Ix data length and discriminator. Allocate signer and buffer authority.

    // 2a) Get offset of IX data
    let mut offset = IX_MIN_OFFSET;
    offset += *(input.add(BUFFER_SIZE) as *const u64) as usize;
    offset += (input.add(offset)).align_offset(ALIGNMENT); // We need to align here, as our account data could be of any length

    // 2b) Get the ix data length
    let mut ix_data_size = *(input.add(offset) as *const u64) as usize;
    offset += size_of::<u64>();

    // 2b) Get discriminator
    let discriminator = *input.add(offset);
    offset += size_of::<u8>();

    // 2c) Allocate signer ID and buffer authority
    let signer: &[u8] = from_raw_parts(input.add(SIGNER_KEY), PUBKEY_LENGTH);
    let buffer_authority = from_raw_parts_mut(input.add(BUFFER_AUTH), PUBKEY_LENGTH);

    // 3. Set up our discriminator and perform additional checks

    // Our instructions include:
    //
    // 0 - Init
    // 1 - Assign
    // 2 - Write
    // 3 - Close

    // Verify the buffer authority for Write, Assign and Close IXs
    if discriminator > 0 && sol_memcmp(buffer_authority, signer, PUBKEY_LENGTH) != 0 {
        log::sol_log("Invalid authority");
        set_return_imm!(1);
        return;
    }

    match discriminator {
        // 0. INIT
        0 => {
            log::sol_log("Init");
            sol_memcpy(buffer_authority, signer, PUBKEY_LENGTH);
            ix_data_size -= size_of::<u8>(); // Remove 1 for the discriminator
            let ix_data: &[u8] = from_raw_parts(input.add(offset), ix_data_size);
            let buffer_data = from_raw_parts_mut(input.add(BUFFER_DATA), ix_data_size);
            sol_memcpy(buffer_data, ix_data, ix_data_size);
        }
        // 1. ASSIGN
        1 => {
            log::sol_log("Assign");
            let new_authority: &[u8] = from_raw_parts(input.add(offset), PUBKEY_LENGTH);
            sol_memcpy(buffer_authority, new_authority, PUBKEY_LENGTH);
        }
        // 2. WRITE
        2 => {
            log::sol_log("Write");
            // Get the offset
            ix_data_size -= size_of::<u32>(); // Remove 1 for discriminator and 3 for u24 offset
            let mut data_offset = *(input.add(offset) as *const u64) as usize;
            data_offset &= U24_MASK;
            data_offset += BUFFER_DATA;
            offset += size_of::<u24>(); // Based u24 hack?

            let ix_data: &[u8] = from_raw_parts(input.add(offset), ix_data_size);
            let buffer_data = from_raw_parts_mut(input.add(data_offset), ix_data_size);
            sol_memcpy(buffer_data, ix_data, ix_data_size);
        }
        // 2. CLOSE
        3 => {
            log::sol_log("Close");
            // Get the lamport balance
            let lamports_buffer = *(input.add(BUFFER_LAMPORTS) as *const u64);
            // Transfer lamports balance to signer
            *(input.add(SIGNER_LAMPORTS) as *mut u64) += lamports_buffer;
            // Wipe lamports
            *(input.add(BUFFER_LAMPORTS) as *mut u64) = 0u64;
            // Wipe size
            *(input.add(BUFFER_SIZE) as *mut u64) = 0u64;
            // Set owner to System Program
            std::ptr::write_volatile(input.add(BUFFER_OWNER) as *mut [u8; 32], [0u8; 32]);
        }
        _ => {
            log::sol_log("Invalid IX");
            set_return_imm!(1);
        }
    }
}
