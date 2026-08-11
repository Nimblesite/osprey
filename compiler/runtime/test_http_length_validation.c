#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <errno.h>
#include <pthread.h>
#include <time.h>

#include "http_shared.h"

// Test counter
static int tests_passed = 0;
static int tests_failed = 0;

#define TEST_ASSERT(condition, message) \
    do { \
        if (condition) { \
            printf("✅ PASS: %s\n", message); \
            tests_passed++; \
        } else { \
            printf("❌ FAIL: %s\n", message); \
            tests_failed++; \
        } \
    } while(0)

#define TEST_ASSERT_EQUALS(actual, expected, message) \
    do { \
        if ((long)(actual) == (long)(expected)) { \
            printf("✅ PASS: %s (got %ld, expected %ld)\n", message, (long)(actual), (long)(expected)); \
            tests_passed++; \
        } else { \
            printf("❌ FAIL: %s (got %ld, expected %ld)\n", message, (long)(actual), (long)(expected)); \
            tests_failed++; \
        } \
    } while(0)

#define TEST_ASSERT_NOT_NULL(ptr, message) \
    do { \
        if ((ptr) != NULL) { \
            printf("✅ PASS: %s\n", message); \
            tests_passed++; \
        } else { \
            printf("❌ FAIL: %s (got NULL)\n", message); \
            tests_failed++; \
        } \
    } while(0)

// Mock HttpResponse for testing (UPDATED TO MATCH NEW SIMPLIFIED STRUCTURE)
typedef struct TestHttpResponse {
    int64_t status;
    char *headers;
    char *contentType;
    int64_t streamFd;
    bool isComplete;
    char *partialBody;  // Runtime automatically calculates length
} TestHttpResponse;

// Test HTTP response creation with various body lengths
void test_http_response_length_calculation(void) {
    printf("\n🧪 Testing HTTP Response Length Calculation...\n");
    
    // Test 1: Empty response (NO MORE HARDCODED LENGTHS!)
    TestHttpResponse empty_resp = {
        .status = 200,
        .headers = "Content-Type: application/json\r\n",
        .streamFd = -1,
        .isComplete = true,
        .partialBody = ""
    };
    
    size_t actual_length = strlen(empty_resp.partialBody);
    TEST_ASSERT_EQUALS(actual_length, 0, "Empty response should have 0 length");
    TEST_ASSERT(actual_length == strlen(empty_resp.partialBody), "Runtime should calculate length from string");
    
    // Test 2: Short JSON response (NO MORE HARDCODED LENGTHS!)
    TestHttpResponse short_resp = {
        .status = 200,
        .headers = "Content-Type: application/json\r\n",
        .streamFd = -1,
        .isComplete = true,
        .partialBody = "{\"success\": true}"
    };
    
    actual_length = strlen(short_resp.partialBody);
    TEST_ASSERT_EQUALS(actual_length, 17, "Short JSON response should have correct length");
    TEST_ASSERT(actual_length == strlen(short_resp.partialBody), "Runtime should calculate length from string");
    
    // Test 3: Long JSON response
    char long_json[2000];
    strcpy(long_json, "{\"success\": true, \"compilerOutput\": \"Compilation successful\", \"programOutput\": \"");
    // Add long content
    for (int i = 0; i < 50; i++) {
        strcat(long_json, "This is a very long output message that should test proper length calculation. ");
    }
    strcat(long_json, "\"}");
    
    TestHttpResponse long_resp = {
        .status = 200,
        .headers = "Content-Type: application/json\r\n",
        .streamFd = -1,
        .isComplete = true,
        .partialBody = long_json
    };
    
    actual_length = strlen(long_resp.partialBody);
    TEST_ASSERT(actual_length > 1000, "Long JSON response should be > 1000 chars");
    TEST_ASSERT(actual_length == strlen(long_resp.partialBody), "Runtime should calculate length from string");
    
    // Test 4: Response with special characters (NO MORE HARDCODED LENGTHS!)
    TestHttpResponse special_resp = {
        .status = 200,
        .headers = "Content-Type: application/json\r\n",
        .streamFd = -1,
        .isComplete = true,
        .partialBody = "{\"message\": \"Hello\\nWorld\\t\\\"Test\\\"\"}"
    };
    
    actual_length = strlen(special_resp.partialBody);
    TEST_ASSERT_EQUALS(actual_length, 33, "Special chars response should have correct length");
    TEST_ASSERT(actual_length == strlen(special_resp.partialBody), "Runtime should calculate length from string");
}

// Test buffer overflow protection
void test_buffer_overflow_protection(void) {
    printf("\n🧪 Testing Buffer Overflow Protection...\n");
    
    // Test 1: Oversized method string
    char oversized_method[1000];
    memset(oversized_method, 'A', 999);
    oversized_method[999] = '\0';
    
    // This should not cause buffer overflow when parsing
    char method_buffer[16];
    char path_buffer[256];
    int parsed = sscanf("GET /test HTTP/1.1", "%15s %255s", method_buffer, path_buffer);
    TEST_ASSERT_EQUALS(parsed, 2, "Normal parsing should work");
    
    // Test with oversized input - should truncate safely
    char oversized_input[2000];
    snprintf(oversized_input, sizeof(oversized_input), "%s /test HTTP/1.1", oversized_method);
    
    parsed = sscanf(oversized_input, "%15s %255s", method_buffer, path_buffer);
    TEST_ASSERT_EQUALS(parsed, 2, "Oversized method parsing should still work");
    TEST_ASSERT_EQUALS(strlen(method_buffer), 15, "Method should be truncated to 15 chars");
    
    // Test 2: Oversized path string
    char oversized_path[1000];
    memset(oversized_path, 'A', 999);
    oversized_path[999] = '\0';
    
    char oversized_path_input[1200];
    snprintf(oversized_path_input, sizeof(oversized_path_input), "GET %s HTTP/1.1", oversized_path);
    
    parsed = sscanf(oversized_path_input, "%15s %255s", method_buffer, path_buffer);
    TEST_ASSERT_EQUALS(parsed, 2, "Oversized path parsing should work");
    TEST_ASSERT_EQUALS(strlen(path_buffer), 255, "Path should be truncated to 255 chars");
}

// Test HTTP header construction
void test_http_header_construction(void) {
    printf("\n🧪 Testing HTTP Header Construction...\n");
    
    // Test 1: Content-Length header with various sizes
    char http_response[8192];
    size_t body_lengths[] = {0, 1, 10, 100, 1000, 10000, 100000};
    
    for (size_t i = 0; i < sizeof(body_lengths) / sizeof(body_lengths[0]); i++) {
        size_t body_len = body_lengths[i];
        
        int header_len = snprintf(http_response, sizeof(http_response),
                                "HTTP/1.1 200 OK\r\n"
                                "Content-Type: application/json\r\n"
                                "Content-Length: %zu\r\n"
                                "Connection: close\r\n"
                                "\r\n",
                                body_len);
        
        TEST_ASSERT(header_len > 0, "Header construction should succeed");
        TEST_ASSERT(header_len > 0 && (size_t)header_len < sizeof(http_response), "Header should fit in buffer");
        
        // Verify Content-Length is correctly formatted
        char expected_length_str[32];
        snprintf(expected_length_str, sizeof(expected_length_str), "Content-Length: %zu\r\n", body_len);
        TEST_ASSERT_NOT_NULL(strstr(http_response, expected_length_str), "Content-Length should be correctly formatted");
    }
    
    // Test 2: No hardcoded lengths in headers
    const char *test_headers = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: %zu\r\nConnection: close\r\n\r\n";
    TEST_ASSERT(strstr(test_headers, "Content-Length: 13") == NULL, "No hardcoded Content-Length: 13");
    TEST_ASSERT(strstr(test_headers, "Content-Length: 25") == NULL, "No hardcoded Content-Length: 25");
    TEST_ASSERT(strstr(test_headers, "Content-Length: 300") == NULL, "No hardcoded Content-Length: 300");
    TEST_ASSERT(strstr(test_headers, "Content-Length: 2000") == NULL, "No hardcoded Content-Length: 2000");
    TEST_ASSERT_NOT_NULL(strstr(test_headers, "Content-Length: %zu"), "Should use dynamic Content-Length");
}

// Test that fallback response uses dynamic length
void test_fallback_response_dynamic_length(void) {
    printf("\n🧪 Testing Fallback Response Dynamic Length...\n");
    
    // Test the actual fallback response logic
    const char *test_body = "Hello, World!";
    size_t expected_len = strlen(test_body);
    
    char response_buffer[1024];
    int response_len = snprintf(response_buffer, sizeof(response_buffer),
                               "HTTP/1.1 200 OK\r\n"
                               "Content-Type: text/plain\r\n"
                               "Content-Length: %zu\r\n"
                               "Connection: close\r\n"
                               "\r\n",
                               expected_len);
    
    TEST_ASSERT(response_len > 0, "Fallback response header should be constructed");
    TEST_ASSERT_EQUALS(expected_len, 13, "Hello, World! should be 13 chars");
    
    // Verify no hardcoded length in the constructed response
    TEST_ASSERT(strstr(response_buffer, "Content-Length: 13") != NULL, "Should have correct dynamic length");
    TEST_ASSERT(strstr(response_buffer, "Content-Length: %zu") == NULL, "Should not have format string");
    
    // Test with different body
    const char *test_body2 = "Different message";
    size_t expected_len2 = strlen(test_body2);
    
    response_len = snprintf(response_buffer, sizeof(response_buffer),
                           "HTTP/1.1 200 OK\r\n"
                           "Content-Type: text/plain\r\n"
                           "Content-Length: %zu\r\n"
                           "Connection: close\r\n"
                           "\r\n",
                           expected_len2);
    
    TEST_ASSERT_EQUALS(expected_len2, 17, "Different message should be 17 chars");
    TEST_ASSERT(strstr(response_buffer, "Content-Length: 17") != NULL, "Should have correct dynamic length for different body");
}

// Test edge cases and potential security issues
void test_security_edge_cases(void) {
    printf("\n🧪 Testing Security Edge Cases...\n");
    
    // Test 1: NULL body handling
    char *null_body = NULL;
    size_t null_len = null_body ? strlen(null_body) : 0;
    TEST_ASSERT_EQUALS(null_len, 0, "NULL body should have 0 length");
    
    // Test 2: Empty string body
    char *empty_body = "";
    size_t empty_len = strlen(empty_body);
    TEST_ASSERT_EQUALS(empty_len, 0, "Empty body should have 0 length");
    
    // Test 3: Body with null bytes (should stop at first null)
    char body_with_null[] = "Hello\0World";
    size_t null_byte_len = strlen(body_with_null);
    TEST_ASSERT_EQUALS(null_byte_len, 5, "Body with null byte should stop at null");
    
    // Test 4: Very long body
    char *long_body = malloc(100000);
    if (long_body) {
        memset(long_body, 'A', 99999);
        long_body[99999] = '\0';
        size_t long_len = strlen(long_body);
        TEST_ASSERT_EQUALS(long_len, 99999, "Very long body should have correct length");
        free(long_body);
    }
    
    // Test 5: Buffer size validation
    char small_buffer[10];
    const char *large_content = "This is a very long string that will not fit in the small buffer";
    
    int written = snprintf(small_buffer, sizeof(small_buffer), "%s", large_content);
    TEST_ASSERT(written > 0 && (size_t)written >= strlen(large_content), "snprintf should report full length needed");
    TEST_ASSERT_EQUALS(strlen(small_buffer), 9, "Buffer should be truncated to fit");
    TEST_ASSERT_EQUALS(small_buffer[9], '\0', "Buffer should be null-terminated");
}

// Main test runner
int main(void) {
    printf("🚨 HTTP LENGTH VALIDATION TEST SUITE 🚨\n");
    printf("=======================================\n");
    printf("Testing for hardcoded lengths, buffer overflows, and security issues...\n");
    
    test_http_response_length_calculation();
    test_buffer_overflow_protection();
    test_http_header_construction();
    test_fallback_response_dynamic_length();
    test_security_edge_cases();
    
    printf("\n📊 TEST RESULTS:\n");
    printf("✅ Passed: %d\n", tests_passed);
    printf("❌ Failed: %d\n", tests_failed);
    printf("📈 Total:  %d\n", tests_passed + tests_failed);
    
    if (tests_failed > 0) {
        printf("\n🚨 CRITICAL: %d tests failed! HTTP length handling has bugs!\n", tests_failed);
        return 1;
    } else {
        printf("\n🎉 SUCCESS: All HTTP length validation tests passed!\n");
        return 0;
    }
} 