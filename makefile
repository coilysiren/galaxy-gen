install-dev: # install reloaders, tests, debuggers, etc
	cargo install cargo-watch

start: # start main script, with code reloads
	cargo watch -x check -x run
