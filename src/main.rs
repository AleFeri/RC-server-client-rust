use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};

/// I need a TAI UTC OFFSET because I'm on macos and I don't have CLOCK_TAI
const TAI_UTC_OFFSET: u64 = 37;

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
        /// Enable rtt
        #[arg(long)]
        rtt: bool,
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
        /// Enable rtt
        #[arg(long)]
        rtt: bool,
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
            rtt,
        } => match protocol {
            Protocol::Udp => udp_server(port, transform, rtt),
            Protocol::Tcp => tcp_server(port, transform, rtt),
        },
        Command::Client {
            host,
            port,
            protocol,
            message,
            count,
            rtt,
        } => match protocol {
            Protocol::Udp => udp_client(host, port, message, count, rtt),
            Protocol::Tcp => tcp_client(host, port, message, count, rtt),
        },
    }
}

/*
 * Servers
 */
fn udp_server(port: u16, transform: Transform, rtt: bool) -> std::io::Result<()> {
    let socket = UdpSocket::bind(("0.0.0.0", port))?;
    let mut buf = [0u8; 1472];

    loop {
        let (n, addr) = socket.recv_from(&mut buf)?;
        let response = if rtt && n > 8 {
            let mut out = Vec::with_capacity(n);
            out.extend_from_slice(&buf[..8]);
            out.extend_from_slice(&apply_transform(&buf[8..n], transform));
            out
        } else {
            apply_transform(&buf[..n], transform)
        };
        socket.send_to(&response, addr)?;
    }
}

fn tcp_server(port: u16, transform: Transform, rtt: bool) -> std::io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    for stream in listener.incoming() {
        let stream = stream?;
        thread::spawn(move || {
            if let Err(e) = tcp_server_handle_client(stream, transform, rtt) {
                eprintln!("client error: {e}");
            }
        });
    }
    Ok(())
}

fn tcp_server_handle_client(
    mut stream: TcpStream,
    transform: Transform,
    rtt: bool,
) -> std::io::Result<()> {
    let mut buf = [0u8; 1472];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(()); // conn closed by client
        }
        let response = if rtt && n > 8 {
            let mut out = Vec::with_capacity(n);
            out.extend_from_slice(&buf[..8]);
            out.extend_from_slice(&apply_transform(&buf[8..n], transform));
            out
        } else {
            apply_transform(&buf[..n], transform)
        };
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
fn udp_client(
    host: String,
    port: u16,
    message: String,
    count: u32,
    rtt: bool,
) -> std::io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect((host, port))?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = [0u8; 1472];

    let mut rtts_ms: Vec<f64> = Vec::new();

    for i in 0..count {
        let payload = if rtt {
            let mut p = Vec::with_capacity(8 + message.len());
            p.extend_from_slice(&ptpv2_now());
            p.extend_from_slice(message.as_bytes());
            p
        } else {
            message.as_bytes().to_vec()
        };

        socket.send(&payload)?;

        match socket.recv(&mut buf) {
            Ok(n) => {
                if rtt {
                    let rtt_ms =
                        calculate_rtt(buf[0..8].try_into().unwrap()).as_secs_f64() * 1000.0;
                    rtts_ms.push(rtt_ms);
                    println!(
                        "[{i}] RTT: {:.3} ms, received: {}",
                        rtt_ms,
                        String::from_utf8_lossy(&buf[8..n])
                    );
                } else {
                    println!("[{i}] received: {}", String::from_utf8_lossy(&buf[..n]));
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                eprintln!("[{i}] timeout");
            }
            Err(e) => return Err(e),
        }
    }

    if rtt {
        let avg_ms = rtts_ms.iter().sum::<f64>() / rtts_ms.len() as f64;
        println!("Avg RTT: {:.3} ms", avg_ms);
    }

    Ok(())
}

fn tcp_client(
    host: String,
    port: u16,
    message: String,
    count: u32,
    rtt: bool,
) -> std::io::Result<()> {
    let mut stream = TcpStream::connect((host, port))?;
    let mut buf = [0u8; 1472];

    let mut rtts_ms: Vec<f64> = Vec::new();

    for i in 0..count {
        let payload = if rtt {
            let mut p = Vec::with_capacity(8 + message.len());
            p.extend_from_slice(&ptpv2_now());
            p.extend_from_slice(message.as_bytes());
            p
        } else {
            message.as_bytes().to_vec()
        };

        stream.write_all(&payload)?;

        let n = stream.read(&mut buf)?;

        if rtt {
            let rtt_ms = calculate_rtt(buf[0..8].try_into().unwrap()).as_secs_f64() * 1000.0;
            rtts_ms.push(rtt_ms);
            println!(
                "[{i}] RTT: {:.3} ms, received: {}",
                rtt_ms,
                String::from_utf8_lossy(&buf[8..n])
            );
        } else {
            println!("[{i}] received: {}", String::from_utf8_lossy(&buf[..n]));
        }
    }

    if rtt {
        let avg_ms = rtts_ms.iter().sum::<f64>() / rtts_ms.len() as f64;
        println!("Avg RTT: {:.3} ms", avg_ms);
    }

    Ok(())
}

/// Return the current clock time as RFC 8877 4.3 -> PTPv2 Truncated Timestamp (8 octets, big-endian)
/// We use TAI_UTC_OFFSET because I'm a macos user and i don't have TAI from the system
fn ptpv2_now() -> [u8; 8] {
    let delta = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before 1970");

    let secs = (delta.as_secs() + TAI_UTC_OFFSET) as u32;
    let nanos = delta.subsec_nanos();

    let mut out = [0u8; 8];
    out[0..4].copy_from_slice(&secs.to_be_bytes());
    out[4..8].copy_from_slice(&nanos.to_be_bytes());

    out
}

/// Parse ptpv2 Truncated Timestamp (8 octets, big-endian) into SystemTime
/// Error if nanoseconds are malformed
/// We use TAI_UTC_OFFSET because I'm a macos user and i don't have TAI from the system
fn ptpv2_parse(buf: &[u8; 8]) -> Result<SystemTime, &'static str> {
    let secs = u32::from_be_bytes(buf[0..4].try_into().unwrap());
    let nanos = u32::from_be_bytes(buf[4..8].try_into().unwrap());

    if nanos >= 1_000_000_000 {
        return Err("malformed PTPv2");
    }

    let unix_secs = (secs as u64).saturating_sub(TAI_UTC_OFFSET);
    Ok(UNIX_EPOCH + Duration::new(unix_secs, nanos))
}

/// calculate RTT assiming the buffer starts with PTPv2 Truncated Timestamp
fn calculate_rtt(echoed: &[u8; 8]) -> Duration {
    let sent = ptpv2_parse(echoed).expect("malformed timestamp");
    SystemTime::now()
        .duration_since(sent)
        .unwrap_or(Duration::ZERO)
}
