/* No GPU dependency: exercises discovery limits and repeated connection cleanup. */
#include "outputs.h"
#include <assert.h>
#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static int descriptors(void) {
    DIR *directory = opendir("/proc/self/fd");
    assert(directory);
    int count = 0;
    while (readdir(directory))
        count++;
    closedir(directory);
    return count;
}
int main(int argc, char **argv) {
    if (argc < 3)
        return 2;
    int initial = descriptors();
    for (int i = 0; i < atoi(argv[2]); i++) {
        struct lw_output_snapshot snapshot;
        int result = lw_query_outputs(&snapshot, atoi(argv[1]));
        assert(snapshot.count >= 0 && snapshot.count <= LW_OUTPUT_LIMIT);
        assert(descriptors() == initial);
        if (result) {
            printf("UNAVAILABLE %s\n", snapshot.error);
            return 1;
        }
        int found = argc < 4;
        for (int j = 0; j < snapshot.count; j++) {
            struct lw_output_info *output = &snapshot.outputs[j];
            if (argc >= 4 && !strcmp(output->name, argv[3]))
                found = 1;
            assert(output->scale >= 1);
        }
        assert(found);
    }
    puts("OUTPUT_DISCOVERY_PASS stable_fds=1");
    return 0;
}
