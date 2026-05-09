use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

/// A simple UDP/TCP echo server/client
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Run as server
    Server {
        protocol: Protocol,

        port: u16,

        /// Transformation to apply to the echo before sending it back
        #[arg(short, long, default_value = "none")]
        transform: Transform,
    },
    /// Run as client
    Client {
        protocol: Protocol,

        host: String,

        port: u16,

        message: String,

        /// Number of messages sent
        #[arg(short, long, default_value_t = 1)]
        count: u32,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Protocol {
    #[value(name = "udp", alias = "UDP")]
    Udp,
    #[value(name = "tcp", alias = "TCP")]
    Tcp,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Transform {
    None,
    Upper,
    Reverse,
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Server {
            port,
            protocol,
            transform,
        } => match protocol {
            Protocol::Udp => udp_server(port, transform),
            Protocol::Tcp => tcp_server(port, transform),
        },
        Command::Client {
            host,
            port,
            protocol,
            message,
            count,
        } => match protocol {
            Protocol::Udp => udp_client(host, port, message, count),
            Protocol::Tcp => tcp_client(host, port, message, count),
        },
    }
}

/*
 * Servers
 */
fn udp_server(port: u16, transform: Transform) -> std::io::Result<()> {
    let socket = UdpSocket::bind(("0.0.0.0", port))?;
    let mut buf = [0u8; 1472];

    loop {
        let (n, addr) = socket.recv_from(&mut buf)?;
        let response = apply_transform(&buf[..n], transform);
        socket.send_to(&response, addr)?;
    }
}

fn tcp_server(port: u16, transform: Transform) -> std::io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    for stream in listener.incoming() {
        let stream = stream?;
        thread::spawn(move || {
            if let Err(e) = tcp_server_handle_client(stream, transform) {
                eprintln!("client error: {e}");
            }
        });
    }
    Ok(())
}

fn tcp_server_handle_client(mut stream: TcpStream, transform: Transform) -> std::io::Result<()> {
    let mut buf = [0u8; 1472];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(()); // conn closed by client
        }
        let response = apply_transform(&buf[..n], transform);
        stream.write_all(&response)?;
    }
}

fn apply_transform(input: &[u8], transform: Transform) -> Vec<u8> {
    match transform {
        Transform::None => input.to_vec(),
        Transform::Upper => input.to_ascii_uppercase(),
        Transform::Reverse => {
            let mut v = input.to_vec();
            v.reverse();
            v
        }
    }
}

/*
 * Clients
 */
fn udp_client(host: String, port: u16, message: String, count: u32) -> std::io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect((host, port))?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = [0u8; 1472];

    for i in 0..count {
        socket.send(message.as_bytes())?;
        match socket.recv(&mut buf) {
            Ok(n) => println!("[{i}] received: {}", String::from_utf8_lossy(&buf[..n])),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                eprintln!("[{i}] timeout");
            }
            Err(e) => return Err(e),
        }
    }

    Ok(())
}

fn tcp_client(host: String, port: u16, message: String, count: u32) -> std::io::Result<()> {
    let mut stream = TcpStream::connect((host, port))?;
    let mut buf = [0u8; 1472];

    for i in 0..count {
        stream.write_all(message.as_bytes())?;
        let n = stream.read(&mut buf)?;
        println!("[{i}] received: {}", String::from_utf8_lossy(&buf[..n]));
    }

    Ok(())
}
