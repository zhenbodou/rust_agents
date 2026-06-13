use std::env;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

fn main() {
    if let Err(err) = run() {
        eprintln!("healthcheck failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("127.0.0.1:{port}");
    let timeout = Duration::from_secs(3);
    let socket = addr
        .to_socket_addrs()
        .map_err(|err| format!("resolve {addr}: {err}"))?
        .next()
        .ok_or_else(|| format!("no socket address for {addr}"))?;

    let mut stream =
        TcpStream::connect_timeout(&socket, timeout).map_err(|err| format!("connect: {err}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| format!("set read timeout: {err}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|err| format!("set write timeout: {err}"))?;

    let request = "GET /healthz/ready HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    stream
        .write_all(request.as_bytes())
        .map_err(|err| format!("write request: {err}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| format!("read response: {err}"))?;
    let status_line = response
        .lines()
        .next()
        .ok_or_else(|| "empty response".to_string())?;

    if status_line.contains(" 200 ") {
        Ok(())
    } else {
        Err(format!("unexpected status: {status_line}"))
    }
}
