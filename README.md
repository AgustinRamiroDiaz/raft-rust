# ! Work in progress ! Rust implementation of Raft consensus algorithm

## Why

Studying and learning purposes

## Docs

[Raft algorithm](https://raft.github.io/raft.pdf)

## Usage

Spawn 2 nodes and connect them to each other (run in different terminals)

```bash
RUST_LOG=info cargo run -- -p http://[::1]:50050 -a [::1]:50051
RUST_LOG=info cargo run -- -p http://[::1]:50051 -a [::1]:50050
```

If you have `gnome-terminal` installed you can run the helper script

```sh
make 5terminals
```

# Features

## Current state

- [x] Leader election

## Future ideas

- [ ] Run in Kubernetes
- [ ] Implement key value store

# Technical code ideas

- [ ] Improve code splitting and organization
- [ ] Dependency Injection for testing
