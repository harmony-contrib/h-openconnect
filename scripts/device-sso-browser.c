#include <arpa/inet.h>
#include <errno.h>
#include <netdb.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

struct url_parts {
    char host[256];
    char port[16];
    char path[8192];
};

static int parse_http_url(const char *url, struct url_parts *out)
{
    const char *authority;
    const char *path;
    const char *colon;
    size_t authority_len;

    if (strncmp(url, "http://", 7) != 0)
        return -1;
    authority = url + 7;
    path = strchr(authority, '/');
    if (!path)
        path = authority + strlen(authority);
    authority_len = (size_t)(path - authority);
    colon = memchr(authority, ':', authority_len);
    if (colon) {
        size_t host_len = (size_t)(colon - authority);
        size_t port_len = authority_len - host_len - 1;
        if (!host_len || host_len >= sizeof(out->host) ||
            !port_len || port_len >= sizeof(out->port))
            return -1;
        memcpy(out->host, authority, host_len);
        out->host[host_len] = '\0';
        memcpy(out->port, colon + 1, port_len);
        out->port[port_len] = '\0';
    } else {
        if (!authority_len || authority_len >= sizeof(out->host))
            return -1;
        memcpy(out->host, authority, authority_len);
        out->host[authority_len] = '\0';
        strcpy(out->port, "80");
    }
    if (*path == '\0')
        strcpy(out->path, "/");
    else if (strlen(path) >= sizeof(out->path))
        return -1;
    else
        strcpy(out->path, path);
    return 0;
}

static int connect_host(const char *host, const char *port)
{
    struct addrinfo hints;
    struct addrinfo *addresses = NULL;
    struct addrinfo *candidate;
    int fd = -1;

    if (strcmp(host, "localhost") == 0) {
        struct sockaddr_in6 loopback;
        memset(&loopback, 0, sizeof(loopback));
        loopback.sin6_family = AF_INET6;
        loopback.sin6_port = htons((unsigned short)strtoul(port, NULL, 10));
        loopback.sin6_addr = in6addr_loopback;
        fd = socket(AF_INET6, SOCK_STREAM, 0);
        if (fd >= 0 && connect(fd, (struct sockaddr *)&loopback, sizeof(loopback)) == 0)
            return fd;
        if (fd >= 0)
            close(fd);
        fd = -1;
    }

    memset(&hints, 0, sizeof(hints));
    hints.ai_socktype = SOCK_STREAM;
    if (getaddrinfo(host, port, &hints, &addresses) != 0)
        return -1;
    for (candidate = addresses; candidate; candidate = candidate->ai_next) {
        fd = socket(candidate->ai_family, candidate->ai_socktype, candidate->ai_protocol);
        if (fd >= 0 && connect(fd, candidate->ai_addr, candidate->ai_addrlen) == 0)
            break;
        if (fd >= 0)
            close(fd);
        fd = -1;
    }
    freeaddrinfo(addresses);
    return fd;
}

static int http_get(const char *url, char *response, size_t capacity)
{
    struct url_parts parsed;
    char request[12288];
    int request_len;
    int fd;
    size_t used = 0;

    if (parse_http_url(url, &parsed) != 0)
        return -1;
    fd = connect_host(parsed.host, parsed.port);
    if (fd < 0)
        return -1;
    request_len = snprintf(request, sizeof(request),
        "GET %s HTTP/1.1\r\nHost: %s:%s\r\nConnection: close\r\n\r\n",
        parsed.path, parsed.host, parsed.port);
    if (request_len < 0 || (size_t)request_len >= sizeof(request) ||
        write(fd, request, (size_t)request_len) != request_len) {
        close(fd);
        return -1;
    }
    while (used + 1 < capacity) {
        ssize_t got = read(fd, response + used, capacity - used - 1);
        if (got < 0) {
            if (errno == EINTR)
                continue;
            close(fd);
            return -1;
        }
        if (got == 0)
            break;
        used += (size_t)got;
    }
    close(fd);
    response[used] = '\0';
    return used ? 0 : -1;
}

static int response_location(const char *response, char *location, size_t capacity)
{
    const char *line = response;
    while ((line = strstr(line, "\r\n")) != NULL) {
        size_t length;
        line += 2;
        if (strncasecmp(line, "Location:", 9) != 0)
            continue;
        line += 9;
        while (*line == ' ' || *line == '\t')
            line++;
        length = strcspn(line, "\r\n");
        if (!length || length >= capacity)
            return -1;
        memcpy(location, line, length);
        location[length] = '\0';
        return 0;
    }
    return -1;
}

int main(int argc, char **argv)
{
    char response[32768];
    char callback[16384];

    if (argc != 2) {
        fprintf(stderr, "usage: device-sso-browser http://host:port/path\n");
        return 2;
    }
    if (http_get(argv[1], response, sizeof(response)) != 0 ||
        response_location(response, callback, sizeof(callback)) != 0) {
        fprintf(stderr, "failed to load IdP redirect\n");
        return 1;
    }
    if (strncmp(callback, "http://localhost:29786/api/sso/",
                sizeof("http://localhost:29786/api/sso/") - 1) != 0) {
        fprintf(stderr, "refusing unexpected SSO callback: %s\n", callback);
        return 1;
    }
    if (http_get(callback, response, sizeof(response)) != 0 ||
        (strncmp(response, "HTTP/1.1 302", 12) != 0 &&
         strncmp(response, "HTTP/1.1 200", 12) != 0)) {
        fprintf(stderr, "OpenConnect callback request failed: %.120s\n", response);
        return 1;
    }
    printf("SSO browser callback delivered through device loopback\n");
    return 0;
}
