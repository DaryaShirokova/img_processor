#include <iostream>
#include <fstream>
#include <sys/mman.h>
#include <sys/fcntl.h>
#include <chrono>
#include <thread>

bool read_ppm_to_shared_memory(const std::string& filename, char* shared_arr, int shift) {
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




// Name of the storage that processes images.
const char* STORAGE_ID = "/SHM_IMG_PROCESSOR";
// Assumes metadata + image + response fit into 100kb.
const int STORAGE_SIZE = 100000;

// Address shift for storing image metadata (after sync metadata)
const int IMG_META_SHIFT = 2;
// Address shift for storing the image itself (after metadata)
const int IMG_SHIFT = 4;

const int INPUT = 0;
const int OUTPUT = 1;

const int REQUIRED = 0;
const int READY = 1;

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
    char* addr = (char*)mmap(NULL, STORAGE_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);



    // First two addresses (char) are reserved to synchronize the processes.
    // addr[0] is input data (0 = required, 1 = ready), addr[1] is output data (0 = required, 1 = ready)
    // requestor == image provider (this application), processor = img processor (rust application)
    // When image processor starts (it must start first otherwise memory is not initialized), address is 0 0. It sets it to 0 1 initially.
    // * if requestor sees 0 1, it takes control, reads output (if it requested them before), sets to 0 0, writes image to shared memory and sets to 1 0
    // * if processor sees 1 0, it reads input data, sets to 0 0, calculates output, sets to 0 1.
    // * if processor sees 1 1, no more data expected, processor sets data to 0 0 and ends the process.
    
    // Wait until image processor is ready.
    while (addr[INPUT] != REQUIRED || addr[OUTPUT] != READY) { // wait for 0 1
        std::this_thread::sleep_for(std::chrono::nanoseconds(100));
    }

    addr[OUTPUT] = REQUIRED; // set 0 0


    // Let's process 10 images.
    for (int i = 0; i < 10; i++) {
        std::string filename = "imgs/img" + std::to_string(i) + ".ppm";
        bool read = read_ppm_to_shared_memory(filename, addr, IMG_META_SHIFT);
        if (!read) {
            std::cerr << "Skipping " << filename << std::endl;
            continue;
        }
        
        addr[INPUT] = READY; // 1 0, img processor can take over

        while (addr[INPUT] != REQUIRED || addr[OUTPUT] != READY) { // wait for 0 1 - result ready
            std::this_thread::sleep_for(std::chrono::nanoseconds(100));
        }

        addr[OUTPUT] = REQUIRED; // set 0 0

        // read result
        uint8_t rows_ch = addr[IMG_META_SHIFT];
        uint8_t columns_ch = addr[IMG_META_SHIFT + 1];
        int rows = int(rows_ch);
        int columns = int(columns_ch);
        
        std::cout << "dimensions " << int(rows) << " " << int(columns) << std::endl;
        
        int answer_addr = IMG_SHIFT + 3 * rows * columns;

        std::cout << "img = " << i << std::endl;
        for (int i = 0; i < rows; i++) {
            uint8_t r = addr[answer_addr + 3 * i];
            uint8_t g = addr[answer_addr + 3 * i + 1];
            uint8_t b = addr[answer_addr + 3 * i + 2];
            std::cout << "r" << +i << "= (" << +r << " " << +g << " "  << +b << "); ";
        }
        std::cout << std::endl;
    }

    // no more input expected
    addr[OUTPUT] = 1; // 0 1
    addr[INPUT] = 1; //  1 1

    return 0;
}