#include <iostream>
#include <fstream>
#include <sys/mman.h>
#include <sys/fcntl.h>
#include <chrono>
#include <thread>

// Name of the storage that processes images.
const char* STORAGE_ID = "/SHM_IMG_PROCESSOR";
// Assumes metadata + image + response fit into 100kb.
const int STORAGE_SIZE = 100000;

// Address shift for storing image metadata (after sync metadata)
const int IMG_META_SHIFT = 1;
// Address shift for storing the image itself (after metadata)
const int IMG_SHIFT = IMG_META_SHIFT + 2;

// Synchronizations states
const uint8_t INTERMEDIATE = 0;
const uint8_t OUTPUT_READY = 1;
const uint8_t INPUT_READY = 2;
const uint8_t NO_MORE_INPUT = 3;


bool read_ppm_to_shared_memory(const std::string& filename, volatile char* shared_arr, int shift) {
    std::ifstream ifs(filename);
    if (!ifs) {
        std::cerr << "Can't open file " << filename << std::endl;
        return false;
    }

    // Validate PPM header.
    std::string ppm_format_str;
    std::getline(ifs, ppm_format_str);
    if (ppm_format_str != "P3") {
        std::cerr << "Unexpected symbol in PPM file " << ppm_format_str << std::endl;
        return false;
    }

    // write rows and columns number to share memory
    int rows, columns;
    ifs >> columns;
    ifs >> rows;

    shared_arr[shift] = char(rows);
    shared_arr[shift + 1] = char(columns);

    // maxColour is unused.
    int maxColour;
    ifs >> maxColour;
    

    // Put the image into the shared memory.
    int img_shift = shift + 2;
    for (int i = 0; i < rows * columns; ++i) {
        int r, g, b;
        ifs >> r;
        ifs >> g;
        ifs >> b;
        shared_arr[img_shift + i * 3] = char(r);
        shared_arr[img_shift + i * 3 + 1] = char(g);
        shared_arr[img_shift + i * 3 + 2] = char(b);
    }

    ifs.close();

    return true;
}

int main(int argc, char *argv[])
{


    // Producer (who sends images) assumes image processor has greated the memory segment,
    // thus opening for read-write.
    int fd = shm_open(STORAGE_ID, O_RDWR, S_IRUSR | S_IWUSR);
    if (fd == -1)
    {
        std::perror("Did you forget to run img processor?");
        return 1;
    }
    // No need to ftruncate as image processor initializes its shared memory.
    
    // mmap to shared memory.
    volatile char* sh_addr = (char*)mmap(NULL, STORAGE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);

    // First char (last two bits) is reserved to synchronize the processes.
    // requestor == image provider (this application), processor = img processor (rust application)
    // When image processor starts (it must start first otherwise memory is not initialized), address is INTERMEDIATE. It sets it to OUTPUT_READY initially.
    // * if requestor sees OUTPUT_READY, it takes control, reads output (if it requested it before), sets to INTERMEDIATE, writes image to shared memory and sets to INPUT_READY
    // * if processor sees INPUT_READY, it reads input data, sets to INTERMEDIATE, calculates output, sets to OUTPUT_READY.
    // * if processor sees NO_MORE_INPUT, no more data expected, processor sets data to INTERMEDIATE and ends the process.
    
    // Wait until image processor is ready.
    while (sh_addr[0] != OUTPUT_READY) {
        std::this_thread::sleep_for(std::chrono::nanoseconds(100));
    }

    sh_addr[0] = INTERMEDIATE;

    // Let's process 10 images.
    for (int i = 0; i < 10; i++) {
        std::string filename = "imgs/img" + std::to_string(i) + ".ppm";
        bool read = read_ppm_to_shared_memory(filename, sh_addr, IMG_META_SHIFT);
        if (!read) {
            std::cerr << "Skipping " << filename << std::endl;
            continue;
        }
        
        sh_addr[0] = INPUT_READY; // img processor can take over

        while (sh_addr[0] != OUTPUT_READY) {
            std::this_thread::sleep_for(std::chrono::nanoseconds(100));
        }

        sh_addr[0] = INTERMEDIATE;

        // read result
        uint8_t rows_ch = sh_addr[IMG_META_SHIFT];
        uint8_t columns_ch = sh_addr[IMG_META_SHIFT + 1];
        int rows = int(rows_ch);
        int columns = int(columns_ch);
        
        std::cout << "dimensions " << int(rows) << " " << int(columns) << std::endl;
        
        int answer_addr = IMG_SHIFT + 3 * rows * columns;

        std::cout << "img = " << i << std::endl;
        for (int i = 0; i < rows; i++) {
            uint8_t r = sh_addr[answer_addr + 3 * i];
            uint8_t g = sh_addr[answer_addr + 3 * i + 1];
            uint8_t b = sh_addr[answer_addr + 3 * i + 2];
            std::cout << "r" << +i << "= (" << +r << " " << +g << " "  << +b << "); ";
        }
        std::cout << std::endl;
    }

    sh_addr[0] = NO_MORE_INPUT;

    return 0;
}