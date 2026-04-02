use libc;
use std::io::Error;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixListener;
use std::time::Instant;
use vsock::VsockListener;

fn main() -> Result<(), Error> {
    let repeat: u32 = std::env::var("REPEAT")
        .unwrap_or("1".into())
        .parse()
        .expect("REPEAT env var must be an integer");
    let addr = std::env::args()
        .nth(1)
        .unwrap_or("tcp:127.0.0.1:8000".into());
    let raw_fd;
    if addr.starts_with("unix:") {
        let listener = UnixListener::bind(&addr[5..].to_string())?;
        eprintln!("Listening on: {addr}");
        listener.set_nonblocking(true)?;
        raw_fd = listener.as_raw_fd();
        std::mem::forget(listener);
    } else if addr.starts_with("tcp:") {
        let listener = TcpListener::bind(&addr[4..].to_string())?;
        eprintln!("Listening on: {addr}");
        listener.set_nonblocking(true)?;
        raw_fd = listener.as_raw_fd();
        std::mem::forget(listener);
    } else if addr.starts_with("vsock:") {
        let pos = addr.rfind(":").unwrap();
        let cid: u32 = addr[6..pos].parse().expect("Bad cid");
        let port: u32 = addr[pos + 1..].parse().expect("Bad port");
        let listener = VsockListener::bind_with_cid_port(cid, port)?;
        eprintln!("Listening on: {addr}");
        listener.set_nonblocking(true)?;
        raw_fd = listener.as_raw_fd();
        std::mem::forget(listener);
    } else {
        return Err(Error::from(ErrorKind::InvalidInput));
    }
    for _i in 0..repeat {
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
    }
    Ok(())
}
