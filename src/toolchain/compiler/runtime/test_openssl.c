#include <openssl/sha.h>
#include <stdio.h>

int main() {
    SHA_CTX ctx;
    SHA1_Init(&ctx);
    printf("OpenSSL works\n");
    return 0;
} 