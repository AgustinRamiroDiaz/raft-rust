.PHONY: 5terminals
.ONESHELL:
5terminals:
	export RUST_LOG=info
	gnome-terminal --tab -e "cargo run -- -a [::1]:50000 -p http://[::1]:50001 -p http://[::1]:50002 -p http://[::1]:50003 -p http://[::1]:50004" &
	gnome-terminal --tab -e "cargo run -- -a [::1]:50001 -p http://[::1]:50000 -p http://[::1]:50002 -p http://[::1]:50003 -p http://[::1]:50004" &
	gnome-terminal --tab -e "cargo run -- -a [::1]:50002 -p http://[::1]:50000 -p http://[::1]:50001 -p http://[::1]:50003 -p http://[::1]:50004" &
	gnome-terminal --tab -e "cargo run -- -a [::1]:50003 -p http://[::1]:50000 -p http://[::1]:50001 -p http://[::1]:50002 -p http://[::1]:50004" &
	gnome-terminal --tab -e "cargo run -- -a [::1]:50004 -p http://[::1]:50000 -p http://[::1]:50001 -p http://[::1]:50002 -p http://[::1]:50003" &
