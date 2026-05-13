use std::io::{ErrorKind, Read, Result, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};

const NTP_UNIX_OFFSET: u64 = 2_208_988_800; // seconds from 1900 to 1970

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
        /// Only for TCP (udp ignores it)
        #[arg(long)]
        no_delay: bool,
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
        /// Only for TCP (udp ignores it)
        #[arg(long)]
        no_delay: bool,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Server {
            port,
            protocol,
            transform,
            rtt,
            no_delay,
        } => match protocol {
            Protocol::Udp => udp_server(port, transform, rtt),
            Protocol::Tcp => tcp_server(port, transform, rtt, no_delay),
        },
        Command::Client {
            host,
            port,
            protocol,
            message,
            count,
            rtt,
            no_delay,
        } => match protocol {
            Protocol::Udp => udp_client(host, port, message, count, rtt),
            Protocol::Tcp => tcp_client(host, port, message, count, rtt, no_delay),
        },
    }
}

/*
 * Servers
 */
fn udp_server(port: u16, transform: Transform, rtt: bool) -> Result<()> {
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

fn tcp_server(port: u16, transform: Transform, rtt: bool, no_delay: bool) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    for stream in listener.incoming() {
        let stream = stream?;
        thread::spawn(move || {
            if let Err(e) = tcp_server_handle_client(stream, transform, rtt, no_delay) {
                if e.kind() == ErrorKind::ConnectionReset {
                    println!("client interrupted the connection");
                } else {
                    eprintln!("client error: {e}");
                }
            }
        });
    }
    Ok(())
}

fn tcp_server_handle_client(
    mut stream: TcpStream,
    transform: Transform,
    rtt: bool,
    no_delay: bool,
) -> Result<()> {
    stream.set_nodelay(no_delay)?;

    while let Some(payload) = read_frame(&mut stream)? {
        let response = if rtt && payload.len() > 8 {
            let mut out = Vec::with_capacity(payload.len());
            out.extend_from_slice(&payload[..8]);
            out.extend_from_slice(&apply_transform(&payload[8..], transform));
            out
        } else {
            apply_transform(&payload, transform)
        };

        write_frame(&mut stream, &response)?;
    }

    Ok(())
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
fn udp_client(host: String, port: u16, message: String, count: u32, rtt: bool) -> Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect((host, port))?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = [0u8; 1472];

    let mut sent = 0u32;
    let mut received = 0u32;
    let mut rtts_ms: Vec<f64> = Vec::new();

    for i in 0..count {
        let payload = if rtt {
            let mut p = Vec::with_capacity(8 + message.len());
            p.extend_from_slice(&ntp64_now());
            p.extend_from_slice(message.as_bytes());
            p
        } else {
            message.as_bytes().to_vec()
        };

        socket.send(&payload)?;
        sent += 1;

        match socket.recv(&mut buf) {
            Ok(n) => {
                if rtt {
                    if n < 8 {
                        eprintln!("[{i}] invalid RTT response: < 8 bytes");
                        continue;
                    }

                    let rtt_ms =
                        calculate_rtt(buf[0..8].try_into().unwrap()).as_secs_f64() * 1000.0;
                    received += 1;
                    rtts_ms.push(rtt_ms);
                    println!(
                        "[{i}] RTT: {:.3} ms, received: {}",
                        rtt_ms,
                        String::from_utf8_lossy(&buf[8..n])
                    );
                } else {
                    received += 1;
                    println!("[{i}] received: {}", String::from_utf8_lossy(&buf[..n]));
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionRefused) => {
                eprintln!("[{i}] unanswered");
            }
            Err(e) => return Err(e),
        }
    }

    if rtt {
        print_rtt_stats(&rtts_ms);
    }
    print_client_stats(sent, received);

    Ok(())
}

fn tcp_client(
    host: String,
    port: u16,
    message: String,
    count: u32,
    rtt: bool,
    no_delay: bool,
) -> Result<()> {
    let mut stream = TcpStream::connect((host, port))?;
    stream.set_nodelay(no_delay)?;

    let mut sent = 0u32;
    let mut received = 0u32;
    let mut rtts_ms: Vec<f64> = Vec::new();

    for i in 0..count {
        let payload = if rtt {
            let mut p = Vec::with_capacity(8 + message.len());
            p.extend_from_slice(&ntp64_now());
            p.extend_from_slice(message.as_bytes());
            p
        } else {
            message.as_bytes().to_vec()
        };

        write_frame(&mut stream, &payload)?;
        sent += 1;

        let Some(response) = read_frame(&mut stream)? else {
            eprintln!("[{i}] server closed connection");
            break;
        };

        if rtt {
            if response.len() < 8 {
                eprintln!("[{i}] invalid RTT response: < 8 bytes");
                continue;
            }

            let rtt_ms = calculate_rtt(response[0..8].try_into().unwrap()).as_secs_f64() * 1000.0;
            received += 1;
            rtts_ms.push(rtt_ms);
            println!(
                "[{i}] RTT: {:.3} ms, received: {}",
                rtt_ms,
                String::from_utf8_lossy(&response[8..])
            );
        } else {
            received += 1;
            println!("[{i}] received: {}", String::from_utf8_lossy(&response));
        }
    }

    if rtt {
        print_rtt_stats(&rtts_ms);
    }
    print_client_stats(sent, received);

    Ok(())
}

/*
 * TCP helper functions
 */
fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<()> {
    let len = payload.len() as u32;

    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(payload);

    stream.write_all(&frame)?;

    Ok(())
}

fn read_frame(stream: &mut TcpStream) -> Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];

    match stream.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
            return Ok(None);
        }
        Err(e) => return Err(e),
    }

    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];

    stream.read_exact(&mut payload)?;

    Ok(Some(payload))
}

/*
 * Time stamp helper functions
 */
fn ntp64_now() -> [u8; 8] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch");

    let seconds = now.as_secs() + NTP_UNIX_OFFSET;
    let fraction = ((now.subsec_nanos() as u128) << 32) / 1_000_000_000u128;

    let mut out = [0u8; 8];
    out[0..4].copy_from_slice(&(seconds as u32).to_be_bytes());
    out[4..8].copy_from_slice(&(fraction as u32).to_be_bytes());
    out
}

fn ntp64_parse(buf: &[u8; 8]) -> SystemTime {
    let seconds = u32::from_be_bytes(buf[0..4].try_into().unwrap()) as u64;
    let fraction = u32::from_be_bytes(buf[4..8].try_into().unwrap()) as u128;

    let unix_secs = seconds.saturating_sub(NTP_UNIX_OFFSET);
    let nanos = ((fraction * 1_000_000_000u128) >> 32) as u32;

    UNIX_EPOCH + Duration::new(unix_secs, nanos)
}

/// calculate RTT assuming the buffer starts with NTP64
fn calculate_rtt(echoed: &[u8; 8]) -> Duration {
    SystemTime::now()
        .duration_since(ntp64_parse(echoed))
        .unwrap_or(Duration::ZERO)
}

/*
 * Generic helper functions
 */
fn print_rtt_stats(rtts_ms: &[f64]) {
    if rtts_ms.is_empty() {
        println!("Avg RTT: unavailable (no valid responses)");
        println!("Max RTT: unavailable (no valid responses)");
        return;
    }

    let avg_ms = rtts_ms.iter().sum::<f64>() / rtts_ms.len() as f64;
    let max_ms = rtts_ms.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    println!("Avg RTT: {:.3} ms", avg_ms);
    println!("Max RTT: {:.3} ms", max_ms);
}

fn print_client_stats(sent: u32, received: u32) {
    println!("Sent: {sent}");
    println!("Received: {received}");
    println!("Unanswered: {}", sent - received);
}
