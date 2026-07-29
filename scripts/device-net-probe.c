/*
 * Minimal OpenHarmony VPN policy-routing probe.
 *
 * Run as root and pass the target application UID. The probe drops to that
 * UID before resolving/connecting so NetManager applies the same per-UID VPN
 * rules as it does to the application, instead of HDC's root-shell bypass.
 */
#include <arpa/inet.h>
#include <errno.h>
#include <grp.h>
#include <netdb.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/types.h>
#include <unistd.h>

static void usage(const char *program)
{
    fprintf(stderr, "usage: %s UID HOST [PORT]\n", program);
}

int main(int argc, char **argv)
{
    if (argc != 3 && argc != 4) {
        usage(argv[0]);
        return 2;
    }

    char *end = NULL;
    unsigned long uid_value = strtoul(argv[1], &end, 10);
    if (!end || *end || uid_value > 0xffffffffUL) {
        fprintf(stderr, "invalid uid: %s\n", argv[1]);
        return 2;
    }

    if (setgroups(0, NULL) != 0 ||
        setgid((gid_t)uid_value) != 0 ||
        setuid((uid_t)uid_value) != 0) {
        fprintf(stderr, "drop uid failed: %s\n", strerror(errno));
        return 3;
    }

    const char *host = argv[2];
    const char *port = argc == 4 ? argv[3] : NULL;
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = port ? SOCK_STREAM : 0;

    struct addrinfo *addresses = NULL;
    int gai = getaddrinfo(host, port, &hints, &addresses);
    if (gai != 0) {
        fprintf(stderr, "uid=%lu resolve %s failed: %s\n",
                uid_value, host, gai_strerror(gai));
        return 4;
    }

    int connected = port ? 0 : 1;
    for (struct addrinfo *address = addresses; address; address = address->ai_next) {
        char numeric[INET6_ADDRSTRLEN] = {0};
        void *raw = NULL;
        if (address->ai_family == AF_INET) {
            raw = &((struct sockaddr_in *)address->ai_addr)->sin_addr;
        } else if (address->ai_family == AF_INET6) {
            raw = &((struct sockaddr_in6 *)address->ai_addr)->sin6_addr;
        }
        if (raw && inet_ntop(address->ai_family, raw, numeric, sizeof(numeric))) {
            printf("uid=%lu resolved %s -> %s\n", uid_value, host, numeric);
        }
        if (!port || connected) {
            continue;
        }

        int fd = socket(address->ai_family, address->ai_socktype, address->ai_protocol);
        if (fd < 0) {
            continue;
        }
        struct timeval timeout = {.tv_sec = 5, .tv_usec = 0};
        (void)setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
        (void)setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
        if (connect(fd, address->ai_addr, address->ai_addrlen) == 0) {
            printf("uid=%lu connected %s:%s via %s\n",
                   uid_value, host, port, numeric[0] ? numeric : "unknown");
            connected = 1;
        }
        close(fd);
    }
    freeaddrinfo(addresses);

    if (port && !connected) {
        fprintf(stderr, "uid=%lu connect %s:%s failed\n", uid_value, host, port);
        return 5;
    }
    return 0;
}
