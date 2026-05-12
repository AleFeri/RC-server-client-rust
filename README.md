# Project for RC Unifi 2025-26

A small Rust command-line UDP/TCP echo client and server.

The application can:

- send and receive UDP datagrams;
- open TCP client/server connections;
- echo payloads unchanged or transformed;
- send a configurable number of messages;
- optionally include a timestamp and measure RTT;
- optionally disable Nagle's algorithm for TCP.

## Setup
To compile this project you need Rust and Cargo installed. If you do not have them, you can install Rust with:

```sh
curl https://sh.rustup.rs -sSf | sh
```

After the installation, check that `cargo` works:

```sh
cargo version
```

## Release build

```sh
cargo build --release
```

The executable will be:

```text
target/release/project
```

## Run

Get general help:

```sh
target/release/project --help
```

Server syntax:

```sh
target/release/project server [OPTIONS] <PROTOCOL> <PORT>
```

Client syntax:

```sh
target/release/project client [OPTIONS] <PROTOCOL> <HOST> <PORT> <MESSAGE>
```

Supported protocols:

- `udp`
- `tcp`

## Options

Common options:

- `--rtt`: include an NTP64 timestamp in the payload and measure round-trip time.
- `--no-delay`: TCP only; enables `TCP_NODELAY`, disabling Nagle's algorithm.

Server options:

- `--transform none`: echo the payload unchanged.
- `--transform upper`: convert the echoed payload to uppercase.
- `--transform reverse`: reverse the echoed payload.

Client options:

- `--count <N>` or `-c <N>`: number of messages to send.

## UDP examples

Start a UDP server on port `8888`:

```sh
target/release/project server udp 8888
```

Send 5 UDP messages:

```sh
target/release/project client udp 127.0.0.1 8888 ciao --count 5
```

Start a UDP server that converts replies to uppercase:

```sh
target/release/project server udp 8888 --transform upper
```

Send UDP messages and measure RTT:

```sh
target/release/project server udp 8888 --rtt
target/release/project client udp 127.0.0.1 8888 ciao --count 10 --rtt
```

## TCP examples

Start a TCP server on port `9999`:

```sh
target/release/project server tcp 9999
```

Send 5 TCP messages:

```sh
target/release/project client tcp 127.0.0.1 9999 ciao --count 5
```

Start a TCP server that reverses replies:

```sh
target/release/project server tcp 9999 --transform reverse
```

Send TCP messages and measure RTT:

```sh
target/release/project server tcp 9999 --rtt
target/release/project client tcp 127.0.0.1 9999 ciao --count 10 --rtt
```

Run TCP with Nagle's algorithm disabled:

```sh
target/release/project server tcp 9999 --rtt --no-delay
target/release/project client tcp 127.0.0.1 9999 ciao --count 10 --rtt --no-delay
```

## TCP framing

UDP preserves datagram boundaries, so one UDP datagram is one application message.

TCP is stream-oriented and does not preserve application message boundaries. For this reason, this application uses a simple length-prefixed TCP framing format:

```text
[4-byte big-endian payload length][payload bytes]
```

Both TCP peers must use this framing to interoperate with this application.

## Client output

When `--rtt` is enabled, the client prints the RTT for each valid reply and then prints summary statistics:

```text
Avg RTT: ...
Max RTT: ...
Sent: ...
Received: ...
Unanswered: ...
```

`Unanswered` is the number of sent application messages without a valid reply. For UDP this may indicate packet loss or timeout. For TCP this means a missing application-level response, since TCP retransmissions are handled by the transport layer.
