use std::io::Error;
use std::io::ErrorKind;
use std::net::Shutdown;
use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd;
use std::time::Instant;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::io::unix::AsyncFd;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or("tcp:127.0.0.1:8000".into());
    if addr.starts_with("unix:") {
        let listener = tokio::net::UnixListener::bind(&addr[5..].to_string()).unwrap();
        eprintln!("Listening on: {addr}");
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                if let Err(e) = process(&mut stream).await {
                    eprintln!("failed to process connection; error = {e}");
                }
                stream.shutdown().await.unwrap_or_default();
            });
        }
    } else if addr.starts_with("tcp:") {
        let listener = tokio::net::TcpListener::bind(&addr[4..].to_string())
            .await
            .unwrap();
        eprintln!("Listening on: {addr}");
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                if let Err(e) = process(&mut stream).await {
                    eprintln!("failed to process connection; error = {e}");
                }
                stream.shutdown().await.unwrap_or_default();
            });
        }
    } else if addr.starts_with("vsock:") {
        let pos = addr.rfind(":").unwrap();
        let cid: u32 = addr[6..pos].parse().expect("Bad cid");
        let port: u32 = addr[pos + 1..].parse().expect("Bad port");
        // let listener = tokio_vsock::VsockListener::bind(tokio_vsock::VsockAddr::new(cid, port)).unwrap();
        // eprintln!("Listening on: {addr}");
        // loop {
        //     let (mut stream, _) = listener.accept().await.unwrap();
        //     tokio::spawn(async move {
        //         if let Err(e) = process(&mut stream).await {
        //             eprintln!("failed to process connection; error = {e}");
        //         }
        //         stream.shutdown(Shutdown::Write).unwrap_or_default();
        //     });
        // }
        let listener = vsock::VsockListener::bind_with_cid_port(cid, port).unwrap();
        listener.set_nonblocking(true).unwrap();
        eprintln!("Listening on: {addr}");
        let raw_fd = listener.as_raw_fd();
        let async_fd = AsyncFd::new(raw_fd).unwrap();
        loop {
            let mut guard = async_fd.readable().await.unwrap();
            // Changing this `if` to a `while` matches second accept and makes it slow
            // loop {
            //     let start = Instant::now();
            //     let result = listener.accept();
            //     let duration = start.elapsed();
            //     println!("accept {} us (ok={})", duration.as_micros(), result.is_ok());
            //     if !result.is_ok() {
            //         break;
            //     }
            //     let (std_stream, _std_addr) = result.unwrap();
            //     let mut stream = tokio_vsock::VsockStream::new(std_stream).unwrap();
            //     tokio::spawn(async move {
            //         if let Err(e) = process(&mut stream).await {
            //             eprintln!("failed to process connection; error = {e}");
            //         }
            //         stream.shutdown(Shutdown::Write).unwrap_or_default();
            //     });
            // }
            // Using accept4 directly in a loop is also slow
            loop {
                let start = Instant::now();
                let stream_fd = unsafe {
                    use std::ptr::null_mut;
                    libc::accept4(
                        raw_fd,
                        null_mut(),
                        null_mut(),
                        libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                    )
                };
                println!(
                    "accept {} us (result={})",
                    start.elapsed().as_micros(),
                    stream_fd
                );
                if stream_fd < 0 {
                    break;
                }
                let std_stream = unsafe { vsock::VsockStream::from_raw_fd(stream_fd) };
                let mut stream = tokio_vsock::VsockStream::new(std_stream).unwrap();
                tokio::spawn(async move {
                    if let Err(e) = process(&mut stream).await {
                        eprintln!("failed to process connection; error = {e}");
                    }
                    stream.shutdown(Shutdown::Write).unwrap_or_default();
                });
            }
            guard.clear_ready();
        }
    } else {
        panic!("Invalid input");
    }
}

async fn process<Stream: AsyncRead + AsyncWrite + Unpin>(stream: &mut Stream) -> Result<(), Error> {
    let mut offset = 0;
    let mut buf = [0; 4096];
    loop {
        loop {
            let bytes_read = stream.read(&mut buf[offset..]).await?;
            if bytes_read == 0 {
                return if offset == 0 {
                    Ok(())
                } else {
                    Err(Error::from(ErrorKind::UnexpectedEof))
                };
            }
            offset += bytes_read;
            let mut headers = [httparse::EMPTY_HEADER; 16];
            let mut req: httparse::Request<'_, '_> = httparse::Request::new(&mut headers);
            match req.parse(&buf[..offset]) {
                Ok(httparse::Status::Complete(bytes_consumed)) => {
                    let version = req.version.unwrap();
                    let mut close = version == 0;
                    let mut has_body = false;
                    if req.method != Some("GET") {
                        stream
                            .write_all(
                                b"HTTP/1.1 405 Method Not Allowed\r\n\
                            Connection: close\r\n\
                            Content-Type: text/plain; charset=utf-8\r\n\
                            \r\n\
                            Method Not Allowed",
                            )
                            .await?;
                        return Err(Error::from(ErrorKind::InvalidData));
                    }
                    for header in &*req.headers {
                        if header.name.eq_ignore_ascii_case("connection") {
                            close = !header.value.eq_ignore_ascii_case(b"keep-alive");
                        } else if header.name.eq_ignore_ascii_case("content-length") {
                            has_body = has_body || header.value != b"0";
                        } else if header.name.eq_ignore_ascii_case("transfer-encoding") {
                            has_body = has_body || header.value.eq_ignore_ascii_case(b"chunked");
                        }
                    }
                    if has_body {
                        stream
                            .write_all(
                                b"HTTP/1.1 400 Bad Request\r\n\
                            Connection: close\r\n\
                            Content-Type: text/plain; charset=utf-8\r\n\
                            \r\n\
                            Bad Request",
                            )
                            .await?;
                        return Err(Error::from(ErrorKind::InvalidData));
                    }
                    let body = "Hello, World!";
                    let conn = if close { "close" } else { "keep-alive" };
                    let length = body.len();
                    stream
                        .write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\n\
                        Connection: {conn}\r\n\
                        Content-Length: {length}\r\n\
                        Content-Type: text/plain; charset=utf-8\r\n\
                        \r\n\
                        {body}"
                            )
                            .as_bytes(),
                        )
                        .await?;
                    if close {
                        return Ok(());
                    }
                    buf.copy_within(bytes_consumed..offset, 0);
                    offset -= bytes_consumed;
                    break;
                }
                Ok(httparse::Status::Partial) => {
                    if offset == buf.len() {
                        stream
                            .write_all(
                                b"HTTP/1.1 400 Bad Request\r\n\
                            Connection: close\r\n\
                            Content-Type: text/plain; charset=utf-8\r\n\
                            \r\n\
                            Bad Request",
                            )
                            .await?;
                        return Err(Error::from(ErrorKind::InvalidData));
                    }
                    continue;
                }
                Err(error) => {
                    return Err(Error::new(ErrorKind::InvalidData, error));
                }
            }
        }
    }
}
