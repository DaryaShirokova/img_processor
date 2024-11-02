extern crate libc;

use libc::{c_char, c_void, off_t, size_t};
use libc::{close, ftruncate, mmap, munmap, shm_open, shm_unlink};
use libc::{MAP_FAILED, MAP_SHARED, O_CREAT, O_RDWR, PROT_WRITE, S_IRUSR, S_IWUSR};
use std::collections::HashMap;
use std::error::Error;
use std::{ptr, thread, time};

const STORAGE_ID: *const c_char = b"/SHM_IMG_PROCESSOR\0".as_ptr() as *const c_char;
const STORAGE_SIZE: size_t = 100000; // 100kb

// first addresses reserved for metadata
const IMG_SHIFT: usize = 2;

// shared metadata for synchronization
const INPUT: usize = 0;
const OUTPUT: usize = 1;

const REQUIRED: i8 = 0;
const READY: i8 = 1;

fn most_popular_colour(
    sh_addr: *const c_char,
    row_addr: usize,
    columns: usize,
) -> Option<(c_char, c_char, c_char)> {
    let mut colours = HashMap::new();

    let mut col = 0;
    while col < columns {
        let index: usize = row_addr + col * 3;
        unsafe {
            let r = *sh_addr.add(index);
            let g = *sh_addr.add(index + 1);
            let b = *sh_addr.add(index + 2);
            *colours.entry((r, g, b)).or_insert(0) += 1;
        }
        col += 1;
    }

    colours
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(colour, _)| colour)
}

fn calculate_colours(sh_addr: *mut c_char, rows: usize, columns: usize, answer_addr: usize) {
    let mut i = 0;
    while i < rows {
        let colour = most_popular_colour(sh_addr, IMG_SHIFT + i * columns * 3, columns).unwrap(); // panic on error as we don't expect err here

        println!(
            "Most popular colour, row {} : {}, {}, {}",
            i, colour.0 as u8, colour.1 as u8, colour.2 as u8
        );

        unsafe {
            *sh_addr.add(answer_addr + 3 * i) = colour.0 as c_char;
            *sh_addr.add(answer_addr + 3 * i + 1) = colour.1 as c_char;
            *sh_addr.add(answer_addr + 3 * i + 2) = colour.2 as c_char;
        }
        i += 1;
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let (fd, sh_addr) = unsafe {
        // in case previous process didn't finish correctly..
        shm_unlink(STORAGE_ID);

        // Image processor initializes STORAGE_ID with STORAGE_SIZE
        let fd = shm_open(STORAGE_ID, O_RDWR | O_CREAT, (S_IRUSR | S_IWUSR) as size_t);
        if fd == -1 {
            return Err("shm_open failed".into());
        }

        let res = ftruncate(fd, STORAGE_SIZE as off_t);
        if res == -1 {
            return Err("ftruncate failed".into());
        }

        // mmap to shared memory.
        let shared_addr = mmap(ptr::null_mut(), STORAGE_SIZE, PROT_WRITE, MAP_SHARED, fd, 0);
        if shared_addr == MAP_FAILED {
            return Err("mmap failed".into());
        }
        (fd, shared_addr as *mut c_char)
    };

    // First two addresses (char) are reserved to synchronize the processes.
    // addr[0] is input data (0 = required, 1 = ready), addr[1] is output data (0 = required, 1 = ready)
    // requestor == image provider (c++ application), processor = img processor (this application)
    // When image processor starts (it must start first otherwise memory is not initialized), address is 0 0. It sets it to 0 1 initially.
    // * if requestor sees 0 1, it takes control, reads output (if it requested them before), sets to 0 0, writes image to shared memory and sets to 1 0
    // * if processor sees 1 0, it reads input data, sets to 0 0, calculates output, sets to 0 1.
    // * if processor sees 1 1, no more data expected, processor sets data to 0 0 and ends the process.

    let rows = 100;
    let columns = 200;
    let answer_addr = IMG_SHIFT + 3 * rows * columns;

    unsafe {
        // set initial state to 0 1 (as no one requested input, no one reads it)
        *sh_addr.add(INPUT) = REQUIRED;
        *sh_addr.add(OUTPUT) = READY;
    }

    loop {
        thread::sleep(time::Duration::from_nanos(100));

        unsafe {
            if *sh_addr == READY && *sh_addr.add(1) == REQUIRED {
                // wait for 1 0 to start processing
                *sh_addr = REQUIRED; // set 0 0

                // process image
                calculate_colours(sh_addr, rows, columns, answer_addr);

                *sh_addr.add(1) = READY;
            }

            if *sh_addr == READY && *sh_addr.add(1) == READY {
                // 1 1 - terminate process
                break;
            }
        }
    }

    // Clean up and just log in case of error
    unsafe {
        if munmap(sh_addr as *mut c_void, STORAGE_SIZE) == -1 {
            eprintln!("munmap error")
        }
        if shm_unlink(STORAGE_ID) == -1 {
            eprintln!("couldn't unlink")
        }
        if close(fd) == -1 {
            eprintln!("couldn't close file")
        }
    }
    Ok(())
}
