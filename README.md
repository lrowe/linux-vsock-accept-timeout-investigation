# HTTP Server and client examples to demonstrate slow vsock non-blocking accept

A common pattern in epoll network servers is to accept all pending connections
on the non-blocking socket listened on when epoll_wait returns indicating the
socket is ready. An accept syscall is called in a loop until it returns EAGAIN
indicating that the backlog is empty. For AF_INET and AF_UNIX sockets this final
accept syscall returns EAGAIN immediately but on an AF_VSOCK socket an accept
syscall returning EAGAIN takes around 1ms to return while FD returning accept
syscalls take around 10us.

## Steps to reproduce

Run `cargo build --release` and follow along with the examples.

## Minimal repro

Simply calls accept4 on a non-blocking socket in a loop.

### Minimal VSOCK non-blocking accept4

```
$ REPEAT=5 target/release/minimal vsock:1:8000
Listening on: vsock:1:8000
accept 778 us (result=-1)
accept 1006 us (result=-1)
accept 1007 us (result=-1)
accept 995 us (result=-1)
accept 1010 us (result=-1)
```

```
$ strace target/release/minimal vsock:1:8000
...
socket(AF_VSOCK, SOCK_STREAM|SOCK_CLOEXEC, 0) = 3
bind(3, {sa_family=AF_VSOCK, svm_cid=VMADDR_CID_LOCAL, svm_port=0x1f40, svm_flags=0}, 16) = 0
listen(3, 128)                          = 0
...
ioctl(3, FIONBIO, [1])                  = 0
accept4(3, NULL, NULL, SOCK_CLOEXEC|SOCK_NONBLOCK) = -1 EAGAIN (Resource temporarily unavailable)
...
```

[report-minimal-vsock.txt](./report-minimal-vsock.txt)
```
$ sudo trace-cmd record -p function_graph -g vsock_accept -F target/release/minimal vsock:1:8000; \
sudo trace-cmd report > report-minimal-vsock.txt
...
Listening on: vsock:1:8000
accept 787 us (result=-1)
...
```

### Minimal TCP non-blocking accept4

```
$ REPEAT=5 target/release/minimal tcp:127.0.0.1:8000
Listening on: tcp:127.0.0.1:8000
accept 6 us (result=-1)
accept 2 us (result=-1)
accept 2 us (result=-1)
accept 1 us (result=-1)
accept 1 us (result=-1)
```

```
$ strace target/release/minimal tcp:127.0.0.1:8000
...
socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC, IPPROTO_IP) = 3
setsockopt(3, SOL_SOCKET, SO_REUSEADDR, [1], 4) = 0
bind(3, {sa_family=AF_INET, sin_port=htons(8000), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
listen(3, 128)                          = 0
...
ioctl(3, FIONBIO, [1])                  = 0
accept4(3, NULL, NULL, SOCK_CLOEXEC|SOCK_NONBLOCK) = -1 EAGAIN (Resource temporarily unavailable)
...
```

[report-minimal-inet.txt](./report-minimal-inet.txt)
```
$ sudo trace-cmd record -p function_graph -g __sys_accept4 -F target/release/minimal tcp:127.0.0.1:8000; \
sudo trace-cmd report > report-minimal-inet.txt
...
Listening on: tcp:127.0.0.1:8000
accept 12 us (result=-1)
```

## Reduced realistic repro

### Epoll VSOCK server 

With server logging individual non-blocking accept syscall timings.

```
$ target/release/httpserver vsock:1:8000
Listening on: vsock:1:8000
accept 13 us (result=10)
accept 501 us (result=-1)
accept 9 us (result=10)
accept 1819 us (result=-1)
accept 8 us (result=10)
accept 1209 us (result=-1)
accept 11 us (result=10)
accept 1385 us (result=-1)
accept 12 us (result=10)
accept 1143 us (result=-1)
```

```
$ REPEAT=5 target/release/httpclientsync vsock:1:8000
client 785 us
client 2020 us
client 1461 us
client 1605 us
client 1422 us
HTTP/1.1 200 OK
Connection: close
Content-Type: text/plain; charset=utf-8

Hello, World!
```

```
$ strace target/release/httpserver vsock:1:8000
... # Setup
epoll_create1(EPOLL_CLOEXEC)            = 3
...
socket(AF_VSOCK, SOCK_STREAM|SOCK_CLOEXEC, 0) = 9
bind(9, {sa_family=AF_VSOCK, svm_cid=VMADDR_CID_LOCAL, svm_port=0x1f40, svm_flags=0}, 16) = 0
listen(9, 128)                          = 0
ioctl(9, FIONBIO, [1])                  = 0
...
epoll_ctl(5, EPOLL_CTL_ADD, 9, {events=EPOLLIN|EPOLLOUT|EPOLLRDHUP|EPOLLET, data={u32=3190411008, u64=109340172865280}}) = 0
... # Loop
epoll_wait(3, [{events=EPOLLIN, data={u32=3190411008, u64=109340172865280}}], 1024, -1) = 1
accept4(9, NULL, NULL, SOCK_CLOEXEC|SOCK_NONBLOCK) = 10
write(1, "accept 261 us (result=10)\n", 26accept 261 us (result=10)
) = 26
ioctl(10, FIONBIO, [1])                 = 0
epoll_ctl(5, EPOLL_CTL_ADD, 10, {events=EPOLLIN|EPOLLOUT|EPOLLRDHUP|EPOLLET, data={u32=3190411392, u64=109340172865664}}) = 0
accept4(9, NULL, NULL, SOCK_CLOEXEC|SOCK_NONBLOCK) = -1 EAGAIN (Resource temporarily unavailable)
write(1, "accept 2096 us (result=-1)\n", 27accept 2096 us (result=-1)
) = 27
epoll_wait(3, [{events=EPOLLIN|EPOLLOUT|EPOLLRDHUP, data={u32=3190411392, u64=109340172865664}}], 1024, -1) = 1
recvfrom(10, "GET / HTTP/1.1\r\nHost: localhost\r", 32, 0, NULL, NULL) = 32
recvfrom(10, "\nConnection: close\r\n\r\n", 32, 0, NULL, NULL) = 22
recvfrom(10, "", 74, 0, NULL, NULL)     = 0
sendto(10, "HTTP/1.1 200 OK\r\nConnection: clo"..., 92, MSG_NOSIGNAL, NULL, 0) = 92
shutdown(10, SHUT_WR)                   = 0
epoll_ctl(5, EPOLL_CTL_DEL, 10, NULL)   = 0
close(10)                               = 0
```

### Epoll TCP server

Connections are much faster so did not log individual timings.

```
target/release/httpserver tcp:127.0.0.1:8000
Listening on: tcp:127.0.0.1:8000
```

```
$ REPEAT=5 target/release/httpclientsync tcp:127.0.0.1:8000
client 314 us
client 229 us
client 117 us
client 82 us
client 53 us
HTTP/1.1 200 OK
Connection: close
Content-Type: text/plain; charset=utf-8

Hello, World!
```

The strace shows that the same pattern of epoll and non blocking accept calls.

```
$ strace target/release/httpserver tcp:127.0.0.1:8000
... # Setup
epoll_create1(EPOLL_CLOEXEC)            = 3
...
socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC|SOCK_NONBLOCK, IPPROTO_IP) = 9
setsockopt(9, SOL_SOCKET, SO_REUSEADDR, [1], 4) = 0
bind(9, {sa_family=AF_INET, sin_port=htons(8000), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
listen(9, 128)                          = 0
epoll_ctl(5, EPOLL_CTL_ADD, 9, {events=EPOLLIN|EPOLLOUT|EPOLLRDHUP|EPOLLET, data={u32=2910297856, u64=105487307087616}}) = 0
... # Loop
epoll_wait(3, [{events=EPOLLIN, data={u32=2910297856, u64=105487307087616}}], 1024, -1) = 1
accept4(9, {sa_family=AF_INET, sin_port=htons(59314), sin_addr=inet_addr("127.0.0.1")}, [128 => 16], SOCK_CLOEXEC|SOCK_NONBLOCK) = 10
epoll_ctl(5, EPOLL_CTL_ADD, 10, {events=EPOLLIN|EPOLLOUT|EPOLLRDHUP|EPOLLET, data={u32=2910298240, u64=105487307088000}}) = 0
accept4(9, 0x7ffc608fd3d0, [128], SOCK_CLOEXEC|SOCK_NONBLOCK) = -1 EAGAIN (Resource temporarily unavailable)
epoll_wait(3, [{events=EPOLLIN|EPOLLOUT|EPOLLRDHUP, data={u32=2910298240, u64=105487307088000}}], 1024, -1) = 1
recvfrom(10, "GET / HTTP/1.1\r\nHost: localhost\r", 32, 0, NULL, NULL) = 32
recvfrom(10, "\nConnection: close\r\n\r\n", 32, 0, NULL, NULL) = 22
recvfrom(10, "", 74, 0, NULL, NULL)     = 0
sendto(10, "HTTP/1.1 200 OK\r\nConnection: clo"..., 92, MSG_NOSIGNAL, NULL, 0) = 92
shutdown(10, SHUT_WR)                   = 0
epoll_ctl(5, EPOLL_CTL_DEL, 10, NULL)   = 0
close(10)                               = 0
```

### Blocking VSOCK server

As its a blocking server the first accept is slow waiting for me to start the client.

```
$ target/release/httpserversync vsock:1:8000
Listening on: vsock:1:8000
accept took 745577 us
accept took 140 us
accept took 37 us
accept took 25 us
accept took 11 us
```

```
$ REPEAT=5 target/release/httpclientsync vsock:1:8000
client 245 us
client 137 us
client 57 us
client 42 us
client 36 us
HTTP/1.1 200 OK
Connection: close
Content-Type: text/plain; charset=utf-8

Hello, World!
```

```
$ strace target/release/httpserversync vsock:1:8000
... # Setup
socket(AF_VSOCK, SOCK_STREAM|SOCK_CLOEXEC, 0) = 3
bind(3, {sa_family=AF_VSOCK, svm_cid=VMADDR_CID_LOCAL, svm_port=0x1f40, svm_flags=0}, 16) = 0
listen(3, 128)                          = 0
... # Loop
accept(3, {sa_family=AF_VSOCK, svm_cid=VMADDR_CID_LOCAL, svm_port=0x5558a6ec, svm_flags=0}, [16]) = 4
fcntl(4, F_SETFD, FD_CLOEXEC)           = 0
write(2, "accept took ", 12accept took )            = 12
write(2, "731124", 6731124)                   = 6
write(2, " us\n", 4 us
)                    = 4
recvfrom(4, "GET / HTTP/1.1\r\nHost: localhost\r", 32, 0, NULL, NULL) = 32
recvfrom(4, "\nConnection: close\r\n\r\n", 32, 0, NULL, NULL) = 22
recvfrom(4, "", 10, 0, NULL, NULL)      = 0
sendto(4, "HTTP/1.1 200 OK\r\nConnection: clo"..., 92, MSG_NOSIGNAL, NULL, 0) = 92
shutdown(4, SHUT_WR)                    = 0
close(4)                                = 0
```

## ftrace

sudo trace-cmd record -p function_graph -g __sys_accept4 -F target/release/httpserver vsock:1:8000
sudo trace-cmd report > report.txt
