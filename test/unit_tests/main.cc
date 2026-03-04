#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include <stdio.h>
#include <stdarg.h>

// Mocking Tizen dlog_print for unit testing environment
extern "C" {
int __dlog_print(int log_id, int prio, const char* tag, const char* fmt, ...) {
    (void)log_id;
    (void)prio;
    printf("[%s] ", tag);
    va_list ap;
    va_start(ap, fmt);
    vprintf(fmt, ap);
    va_end(ap);
    printf("\n");
    return 0;
}
}

int main(int argc, char** argv) {
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}
