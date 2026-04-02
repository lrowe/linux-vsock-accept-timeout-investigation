use std::io::Error;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::net::Shutdown;
use std::net::TcpListener;
use std::os::unix::net::UnixListener;
use std::time::Instant;
use vsock::VsockListener;

fn main() -> Result<(), Error> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or("tcp:127.0.0.1:8000".into());
    if addr.starts_with("unix:") {
        let listener = UnixListener::bind(&addr[5..].to_string())?;
        eprintln!("Listening on: {addr}");
        loop {
            let start = Instant::now();
            let (mut stream, _) = listener.accept()?;
            eprintln!("accept took {} us", start.elapsed().as_micros());
            if let Err(e) = process(&mut stream) {
                eprintln!("failed to process connection; error = {e}");
            }
            stream.shutdown(Shutdown::Write).unwrap_or_default();
        }
    } else if addr.starts_with("tcp:") {
        let listener = TcpListener::bind(&addr[4..].to_string())?;
        eprintln!("Listening on: {addr}");
        loop {
            let start = Instant::now();
            let (mut stream, _) = listener.accept()?;
            eprintln!("accept took {} us", start.elapsed().as_micros());
            if let Err(e) = process(&mut stream) {
                eprintln!("failed to process connection; error = {e}");
            }
            stream.shutdown(Shutdown::Write).unwrap_or_default();
        }
    } else if addr.starts_with("vsock:") {
        let pos = addr.rfind(":").unwrap();
        let cid: u32 = addr[6..pos].parse().expect("Bad cid");
        let port: u32 = addr[pos + 1..].parse().expect("Bad port");
        let listener = VsockListener::bind_with_cid_port(cid, port)?;
        eprintln!("Listening on: {addr}");
        loop {
            let start = Instant::now();
            let (mut stream, _) = listener.accept()?;
            eprintln!("accept took {} us", start.elapsed().as_micros());
            if let Err(e) = process(&mut stream) {
                eprintln!("failed to process connection; error = {e}");
            }
            stream.shutdown(Shutdown::Write).unwrap_or_default();
        }
    } else {
        Err(Error::from(ErrorKind::InvalidInput))
    }
}

fn process<Stream: Read + Write>(stream: &mut Stream) -> Result<(), Error> {
    let mut offset = 0;
    let mut buf = [0; 4096];
    loop {
        loop {
            let bytes_read = stream.read(&mut buf[offset..])?;
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
                        stream.write_all(
                            b"HTTP/1.1 405 Method Not Allowed\r\n\
                            Connection: close\r\n\
                            Content-Type: text/plain; charset=utf-8\r\n\
                            \r\n\
                            Method Not Allowed",
                        )?;
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
                        stream.write_all(
                            b"HTTP/1.1 400 Bad Request\r\n\
                            Connection: close\r\n\
                            Content-Type: text/plain; charset=utf-8\r\n\
                            \r\n\
                            Bad Request",
                        )?;
                        return Err(Error::from(ErrorKind::InvalidData));
                    }
                    let body = "Hello, World!";
                    let conn = if close { "close" } else { "keep-alive" };
                    let length = body.len();
                    stream.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\n\
                        Connection: {conn}\r\n\
                        Content-Length: {length}\r\n\
                        Content-Type: text/plain; charset=utf-8\r\n\
                        \r\n\
                        {body}"
                        )
                        .as_bytes(),
                    )?;
                    if close {
                        return Ok(());
                    }
                    buf.copy_within(bytes_consumed..offset, 0);
                    offset -= bytes_consumed;
                    break;
                }
                Ok(httparse::Status::Partial) => {
                    if offset == buf.len() {
                        stream.write_all(
                            b"HTTP/1.1 400 Bad Request\r\n\
                            Connection: close\r\n\
                            Content-Type: text/plain; charset=utf-8\r\n\
                            \r\n\
                            Bad Request",
                        )?;
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
