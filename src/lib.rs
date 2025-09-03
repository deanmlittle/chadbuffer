#![cfg_attr(target_os = "solana", feature(asm_experimental_arch))]

use core::{
    mem::size_of,
    slice::{from_raw_parts, from_raw_parts_mut},
};

use pinocchio::{
    log,
    memory::{sol_memcmp, sol_memcpy},
};
use sbpf_asm_macros::set_return_imm;

#[allow(non_camel_case_types)]
type u24 = [u8; 3];

// Alignment
pub const ALIGNMENT: usize = 0x0008;
pub const PUBKEY_LENGTH: usize = 0x0020;

// Bit masks
pub const SIG_MUT_NODUP: u32 = 0x0101ff;
pub const U24_MASK: usize = 0xffffff;

pub const MAX_PERMITTED_DATA_INCREASE: usize = 1_024 * 10;
pub const BPF_ALIGN_OF_U128: usize = 8;

/// # Safety
/// Where we're going, we don't need memory safety.
#[no_mangle]
pub unsafe extern "C" fn entrypoint(input: *mut u8) {
    let mut offset = 0;

    // 1. Account checks

    // By knowing we have 2 accounts and the signer account is a non-dup,
    // we can skip checking the buffer account, as it will fail mutability anyway.

    // 1a) Check we have 2 accounts and signer is a nodup mut signer
    let num_accounts = *(input.add(offset) as *const u64) as usize;
    offset += size_of::<u64>();

    if num_accounts != 2 {
        log::sol_log("Wrong number of accounts");
        set_return_imm!(1);
        return;
    }

    // 1b) If we have 2 accounts and signer is non-dup, we can skip checking the
    // buffer
    if *(input.add(offset) as *const u32) != SIG_MUT_NODUP {
        log::sol_log("Missing signer");
        set_return_imm!(1);
        return;
    }

    // 2. Allocate signer and buffer authority. Get IX data offset, Ix data length
    //    and discriminator.

    // 2a) Allocate signer ID and buffer authority
    let (signer_key, signer_lamports_offset) = {
        // Skip dup_info, is_signer, is_writable, executable
        offset += size_of::<u8>() + size_of::<u8>() + size_of::<u8>() + size_of::<u8>();

        // Skip data_len
        offset += size_of::<u32>();

        let key: &[u8] = from_raw_parts(input.add(offset), PUBKEY_LENGTH);
        offset += PUBKEY_LENGTH;

        // Skip owner
        offset += PUBKEY_LENGTH;

        let lamports_offset = offset;
        offset += size_of::<u64>();

        let data_len = *(input.add(offset) as *const u64) as usize;
        offset += size_of::<u64>();

        offset += data_len + MAX_PERMITTED_DATA_INCREASE;
        offset += (offset as *const u8).align_offset(BPF_ALIGN_OF_U128); // padding

        // Skip rent_epoch
        offset += size_of::<u64>();

        (key, lamports_offset)
    };

    // 2b) Allocate signer ID and buffer authority
    let (
        buffer_owner_offset,
        buffer_lamports_offset,
        buffer_size_offset,
        buffer_authority,
        buffer_data_offset,
    ) = {
        // Skip dup_info, is_signer, is_writable, executable
        offset += size_of::<u8>() + size_of::<u8>() + size_of::<u8>() + size_of::<u8>();

        // Skip data_len
        offset += size_of::<u32>();

        // Skip key
        offset += PUBKEY_LENGTH;

        // Skip owner
        let owner_offset = offset;
        offset += PUBKEY_LENGTH;

        let lamports_offset = offset;
        offset += size_of::<u64>();

        let data_len_offset = offset;
        let data_len = *(input.add(data_len_offset) as *const u64) as usize;
        offset += size_of::<u64>();

        let buffer_authority = from_raw_parts_mut(input.add(offset), PUBKEY_LENGTH);

        let data_offset = offset + PUBKEY_LENGTH;
        offset += data_len + MAX_PERMITTED_DATA_INCREASE;
        offset += (offset as *const u8).align_offset(BPF_ALIGN_OF_U128); // padding

        // Skip rent_epoch
        offset += size_of::<u64>();

        (owner_offset, lamports_offset, data_len_offset, buffer_authority, data_offset)
    };

    // 2c) Get the ix data length
    let mut ix_data_size = *(input.add(offset) as *const u64) as usize;
    offset += size_of::<u64>();

    // 2d) Get discriminator
    let discriminator = *input.add(offset);
    offset += size_of::<u8>();

    // 3. Set up our discriminator and perform additional checks

    // Our instructions include:
    //
    // 0 - Init
    // 1 - Assign
    // 2 - Write
    // 3 - Close

    // Verify the buffer authority for Write, Assign and Close IXs
    if discriminator > 0 && sol_memcmp(buffer_authority, signer_key, PUBKEY_LENGTH) != 0 {
        log::sol_log("Invalid authority");
        set_return_imm!(1);
        return;
    }

    match discriminator {
        // 0. INIT
        0 => {
            log::sol_log("Init");
            if sol_memcmp(buffer_authority, &[0; PUBKEY_LENGTH], PUBKEY_LENGTH) != 0 {
                log::sol_log("The buffer has been initialized");
                set_return_imm!(1);
                return;
            }

            sol_memcpy(buffer_authority, signer_key, PUBKEY_LENGTH);

            // Remove 1 for the discriminator
            ix_data_size -= size_of::<u8>();
            let ix_data: &[u8] = from_raw_parts(input.add(offset), ix_data_size);
            let buffer_data = from_raw_parts_mut(input.add(buffer_data_offset), ix_data_size);
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
            data_offset += buffer_data_offset;
            offset += size_of::<u24>(); // Based u24 hack?

            let ix_data: &[u8] = from_raw_parts(input.add(offset), ix_data_size);
            let buffer_data = from_raw_parts_mut(input.add(data_offset), ix_data_size);
            sol_memcpy(buffer_data, ix_data, ix_data_size);
        }
        // 2. CLOSE
        3 => {
            log::sol_log("Close");
            // Get the lamport balance
            let lamports_buffer = *(input.add(buffer_lamports_offset) as *const u64);
            // Transfer lamports balance to signer
            *(input.add(signer_lamports_offset) as *mut u64) += lamports_buffer;
            // Wipe lamports
            *(input.add(buffer_lamports_offset) as *mut u64) = 0u64;
            // Wipe size
            *(input.add(buffer_size_offset) as *mut u64) = 0u64;
            // Set owner to System Program
            core::ptr::write_volatile(input.add(buffer_owner_offset) as *mut [u8; 32], [0u8; 32]);
        }
        _ => {
            log::sol_log("Invalid IX");
            set_return_imm!(1);
        }
    }
}
