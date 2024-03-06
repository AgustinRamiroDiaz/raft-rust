.PHONY: 5terminals
5terminals:
	gnome-terminal --tab -- bash -c "RUST_LOG=info cargo run -- -a [::1]:50000 -p http://[::1]:50001 -p http://[::1]:50002 -p http://[::1]:50003 -p http://[::1]:50004" &
	gnome-terminal --tab -- bash -c "RUST_LOG=info cargo run -- -a [::1]:50001 -p http://[::1]:50000 -p http://[::1]:50002 -p http://[::1]:50003 -p http://[::1]:50004" &
	gnome-terminal --tab -- bash -c "RUST_LOG=info cargo run -- -a [::1]:50002 -p http://[::1]:50000 -p http://[::1]:50001 -p http://[::1]:50003 -p http://[::1]:50004" &
	gnome-terminal --tab -- bash -c "RUST_LOG=info cargo run -- -a [::1]:50003 -p http://[::1]:50000 -p http://[::1]:50001 -p http://[::1]:50002 -p http://[::1]:50004" &
	gnome-terminal --tab -- bash -c "RUST_LOG=info cargo run -- -a [::1]:50004 -p http://[::1]:50000 -p http://[::1]:50001 -p http://[::1]:50002 -p http://[::1]:50003" &
