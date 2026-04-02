use std::io::Error;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::net::Shutdown;
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::time::Instant;
use vsock::VsockStream;

static HTTP_REQUEST: &'static [u8] = b"\
GET / HTTP/1.1\r\n\
Host: localhost\r\n\
Connection: close\r\n\
\r\n";

fn main() -> Result<(), Error> {
    let repeat: u32 = std::env::var("REPEAT")
        .unwrap_or("1".into())
        .parse()
        .expect("REPEAT env var must be an integer");
    let addr = std::env::args()
        .nth(1)
        .unwrap_or("tcp:127.0.0.1:8000".into());
    let mut res = Vec::new();
    let mut offset = 0;
    for _i in 0..repeat {
        res.clear();
        let elapsed;
        if addr.starts_with("unix:") {
            let start = Instant::now();
            let mut stream = UnixStream::connect(&addr[5..])?;
            stream.write_all(HTTP_REQUEST)?;
            stream.shutdown(Shutdown::Write)?;
            stream.read_to_end(&mut res)?;
            elapsed = start.elapsed();
        } else if addr.starts_with("tcp:") {
            let start = Instant::now();
            let mut stream = TcpStream::connect(&addr[4..])?;
            stream.write_all(HTTP_REQUEST)?;
            stream.shutdown(Shutdown::Write)?;
            stream.read_to_end(&mut res)?;
            elapsed = start.elapsed();
        } else if addr.starts_with("vsock:") {
            let pos = 6 + addr[6..].find(":").unwrap();
            let cid: u32 = addr[6..pos].parse().expect("Bad cid");
            let port: u32 = addr[pos + 1..].parse().expect("Bad port");
            let start = Instant::now();
            let mut stream = VsockStream::connect_with_cid_port(cid, port)?;
            stream.write_all(HTTP_REQUEST)?;
            stream.shutdown(Shutdown::Write)?;
            stream.read_to_end(&mut res)?;
            elapsed = start.elapsed();
        } else if addr.starts_with("vsock+unix:") {
            let pos = 11 + addr[11..].rfind(":").unwrap();
            let path = addr[11..pos].to_string();
            let port: u32 = addr[pos + 1..].parse().expect("Bad port");
            let start = Instant::now();
            let mut stream = UnixStream::connect(&path)?;
            stream.write_all(format!("CONNECT {}\n", port).as_bytes())?;
            // let mut connect_line = Vec::with_capacity(32);
            // let connect_read = stream.read(&mut connect_line)?; // get back 0 lengthresponse
            // if !connect_line.starts_with(b"OK ") {
            //     eprintln!("connect_read={}", connect_read);
            //     std::io::stderr().write_all(&connect_line)?;
            //     return Err(Error::from(ErrorKind::ConnectionRefused));
            // }
            stream.write_all(HTTP_REQUEST)?;
            //stream.shutdown(Shutdown::Write)?; // causes a connection reset
            stream.read_to_end(&mut res)?;
            elapsed = start.elapsed();
            offset = res.iter().position(|&r| r == b'\n').unwrap() + 1;
        } else {
            return Err(Error::from(ErrorKind::InvalidInput));
        }
        eprintln!("client {} us", elapsed.as_micros());
        if !res[offset..].starts_with(b"HTTP/1.1") {
            eprintln!("offset={}", offset);
            std::io::stdout().write_all(&res)?;
            return Err(Error::from(ErrorKind::InvalidData));
        }
    }
    std::io::stdout().write_all(&res[offset..])
}
