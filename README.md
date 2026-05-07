# Project for RC unifi 2025-26
The assignment is in the e-l platform of the course so I'm going to omit it here. Other students will know.

## Setup
To compile this project you need rust and cargo installed. If you don't have it, just run the following command to install it:
```sh
curl https://sh.rustup.rs -sSf | sh
```

After the installation make sure that you can succesfully run `cargo` on your machine. Here is an example with the version i currently have installed.
```
> cargo version
cargo 1.95.0 (f2d3ce0bd 2026-03-21)
```

## Release build
Like every cargo project you just do `cargo build --release` and wait for it.
Again like the other cargo projects the build will go under the `target/release` folder.

## Run
The executable is under `target/release/project`, so to get help do:
```sh
target/release/project --help
```
For the server you do:
```sh
target/release/project server [OPTIONS] <PROTOCOL> <PORT>
```
And for server help:
```sh
target/release/project server --help
```

For client you do:
```sh
target/release/project client [OPTIONS] <PROTOCOL> <HOST> <PORT> <MESSAGE>
```
And for client help:
```sh
target/release/project client --help
```
