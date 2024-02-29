# Rust implementation of Raft consensus algorithm

[Raft algorithm](https://raft.github.io/raft.pdf)

## Usage

Spawn 2 nodes and connect them to each other (run in different terminals)

```bash
RUST_LOG=info cargo run -- -p http://[::1]:50050 -a [::1]:50051
RUST_LOG=info car
go run -- -p http://[::1]:50051 -a [::1]:50050
```
