See `demo.mov` for example of how to run the application (instructions below).

The repository contains two applications:
* img_provider_cpp is a C++ application that reads images from the file and sends them to image processor over the shared memory.
* processor_rust is the image processor that processes the image and writes the result (most common colour) to the shared memory.


Pre-requisites (locations to download provided for mac OS)
1. Install clang if not already available (clang --version to check if available, if not use `xcode-select --install` on mac OS).
1. Install rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`. See https://www.rust-lang.org/tools/install.

To run:
1. Open two terminal windows, go to `cd processor_rust` in the first and `cd img_provider_cpp` in the second.
1. Build processor_rust from processor_rust terminal: `cargo build`.
1. Create build dir from img_provider_cpp: `mkdir build`.
1. Build img_provider_cpp from img_provider_cpp terminal: `clang++ main.cc -o build/image_provider.out`.
1. Run processor_rust from processor_rust terminal (it must be launched first as it initializes the shared memory):  `cargo run`.
1. Run img_provider_cpp: `./build/image_provider.out`.

Rust application uses libc external dependency that is downloaded when you build the project.