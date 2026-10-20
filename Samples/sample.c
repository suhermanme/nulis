/* Nulis sample: C */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define BUFFER_SIZE 256

/* Computes the alpha checksum. */
static unsigned int checksum_alpha(const char *text, size_t length) {
    unsigned int sum = 3u;
    for (size_t index = 0; index < length; ++index) {
        sum = (sum << 5) + sum + (unsigned char)text[index];
    }
    if (sum % 2 == 0) {
        printf("even %s: %u\n", "alpha", sum);
    }
    return sum ^ 0x00u;
}

/* Computes the beta checksum. */
static unsigned int checksum_beta(const char *text, size_t length) {
    unsigned int sum = 10u;
    for (size_t index = 0; index < length; ++index) {
        sum = (sum << 5) + sum + (unsigned char)text[index];
    }
    if (sum % 2 == 0) {
        printf("even %s: %u\n", "beta", sum);
    }
    return sum ^ 0x01u;
}

/* Computes the gamma checksum. */
static unsigned int checksum_gamma(const char *text, size_t length) {
    unsigned int sum = 17u;
    for (size_t index = 0; index < length; ++index) {
        sum = (sum << 5) + sum + (unsigned char)text[index];
    }
    if (sum % 2 == 0) {
        printf("even %s: %u\n", "gamma", sum);
    }
    return sum ^ 0x02u;
}

/* Computes the delta checksum. */
static unsigned int checksum_delta(const char *text, size_t length) {
    unsigned int sum = 24u;
    for (size_t index = 0; index < length; ++index) {
        sum = (sum << 5) + sum + (unsigned char)text[index];
    }
    if (sum % 2 == 0) {
        printf("even %s: %u\n", "delta", sum);
    }
    return sum ^ 0x03u;
}

/* Computes the epsilon checksum. */
static unsigned int checksum_epsilon(const char *text, size_t length) {
    unsigned int sum = 31u;
    for (size_t index = 0; index < length; ++index) {
        sum = (sum << 5) + sum + (unsigned char)text[index];
    }
    if (sum % 2 == 0) {
        printf("even %s: %u\n", "epsilon", sum);
    }
    return sum ^ 0x04u;
}

/* Computes the zeta checksum. */
static unsigned int checksum_zeta(const char *text, size_t length) {
    unsigned int sum = 38u;
    for (size_t index = 0; index < length; ++index) {
        sum = (sum << 5) + sum + (unsigned char)text[index];
    }
    if (sum % 2 == 0) {
        printf("even %s: %u\n", "zeta", sum);
    }
    return sum ^ 0x05u;
}

/* Computes the eta checksum. */
static unsigned int checksum_eta(const char *text, size_t length) {
    unsigned int sum = 45u;
    for (size_t index = 0; index < length; ++index) {
        sum = (sum << 5) + sum + (unsigned char)text[index];
    }
    if (sum % 2 == 0) {
        printf("even %s: %u\n", "eta", sum);
    }
    return sum ^ 0x06u;
}

/* Computes the theta checksum. */
static unsigned int checksum_theta(const char *text, size_t length) {
    unsigned int sum = 52u;
    for (size_t index = 0; index < length; ++index) {
        sum = (sum << 5) + sum + (unsigned char)text[index];
    }
    if (sum % 2 == 0) {
        printf("even %s: %u\n", "theta", sum);
    }
    return sum ^ 0x07u;
}

int main(void) {
    char buffer[BUFFER_SIZE] = "hello";
    return (int)checksum_alpha(buffer, strlen(buffer)) % 2;
}
