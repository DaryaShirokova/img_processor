extern crate libc;

use libc::{c_char, c_void, off_t, size_t};
use libc::{close, ftruncate, mmap, munmap, shm_open, shm_unlink};
use libc::{MAP_FAILED, MAP_SHARED, O_CREAT, O_RDWR, PROT_WRITE, S_IRUSR, S_IWUSR};
use std::collections::HashMap;
use std::error::Error;
use std::{ptr, thread, time};

const STORAGE_ID: *const c_char = b"/SHM_IMG_PROCESSOR\0".as_ptr() as *const c_char;
const STORAGE_SIZE: size_t = 100000; // 100 kilobytes

// first address is reserved for sync metadata, followed by image rows and cols
const IMG_METADATA_SHIFT: usize = 1;
const IMG_SHIFT: usize = IMG_METADATA_SHIFT + 2;

// shared metadata for synchronization
const INRERMEDIATE: i8 = 0;
const OUTPUT_READY: i8 = 1;
const INPUT_READY: i8 = 2;
const NO_MORE_INPUT: i8 = 3;

const SLEEP_NANO: u64 = 100;

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
            let r = ptr::read_volatile(sh_addr.add(index));
            let g = ptr::read_volatile(sh_addr.add(index + 1));
            let b = ptr::read_volatile(sh_addr.add(index + 2));
            *colours.entry((r, g, b)).or_insert(0) += 1;
        }
        col += 1;
    }

    colours
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(colour, _)| colour)
}

fn calculate_colours(sh_addr: *mut c_char) {
    // read image dimensions
    let (rows, columns) = unsafe {
        let rows = ptr::read_volatile(sh_addr.add(IMG_METADATA_SHIFT)) as u8;
        let columns = ptr::read_volatile(sh_addr.add(IMG_METADATA_SHIFT + 1)) as u8;
        (rows as usize, columns as usize)
    };

    // put asnswer after the metadata and image data
    let answer_addr: usize = IMG_SHIFT + 3 * rows * columns;

    let mut i: usize = 0;
    while i < rows {
        let colour = most_popular_colour(sh_addr, IMG_SHIFT + i * columns * 3, columns).unwrap(); // panic on error as we don't expect err here

        println!(
            "Most popular colour, row {} : {}, {}, {}",
            i, colour.0 as u8, colour.1 as u8, colour.2 as u8
        );

        unsafe {
            ptr::write_volatile(sh_addr.add(answer_addr + 3 * i), colour.0 as c_char);
            ptr::write_volatile(sh_addr.add(answer_addr + 3 * i + 1), colour.1 as c_char);
            ptr::write_volatile(sh_addr.add(answer_addr + 3 * i + 2), colour.2 as c_char);
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
            close(fd);
            return Err("ftruncate failed".into());
        }

        // mmap to shared memory.
        let shared_addr = mmap(ptr::null_mut(), STORAGE_SIZE, PROT_WRITE, MAP_SHARED, fd, 0);
        if shared_addr == MAP_FAILED {
            shm_unlink(STORAGE_ID);
            close(fd);
            return Err("mmap failed".into());
        }
        (fd, shared_addr as *mut c_char)
    };

    // First char (last two bits) is reserved to synchronize the processes.
    // requestor == image provider (c++ application), processor = img processor (this application)
    // When image processor starts (it must start first otherwise memory is not initialized), address is INTERMEDIATE. It sets it to OUTPUT_READY initially.
    // * if requestor sees OUTPUT_READY, it takes control, reads output (if it requested it before), sets to INTERMEDIATE, writes image to shared memory and sets to INPUT_READY
    // * if processor sees INPUT_READY, it reads input data, sets to INTERMEDIATE, calculates output, sets to OUTPUT_READY.
    // * if processor sees NO_MORE_INPUT, no more data expected, processor sets data to INTERMEDIATE and ends the process.
    unsafe {
        // set initial state to OUTPUT_READY (as no one requested input, no one reads it)
        ptr::write_volatile(sh_addr, OUTPUT_READY);
    }

    loop {
        thread::sleep(time::Duration::from_nanos(SLEEP_NANO));

        unsafe {
            // wait for INPUT_READY to start processing
            if ptr::read_volatile(sh_addr) == INPUT_READY {
                ptr::write_volatile(sh_addr, INRERMEDIATE);

                // process image
                calculate_colours(sh_addr);

                ptr::write_volatile(sh_addr, OUTPUT_READY);
            }

            if ptr::read_volatile(sh_addr) == NO_MORE_INPUT {
                // NO_MORE_INPUT - terminate process
                ptr::write_volatile(sh_addr, INRERMEDIATE);
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
