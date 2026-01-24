.PHONY: dev release css clean run run-release

CSS_INPUT := style/tailwind.css
CSS_OUTPUT := ../../target/site/pkg/bp-web.css

# Development build
dev: css
	cargo build -p bp-web --features ssr

# Release build
release: css
	cargo build -p bp-web --features ssr --release

# Build CSS with Tailwind v4
css:
	@mkdir -p ../../target/site/pkg
	npx @tailwindcss/cli --input $(CSS_INPUT) --output $(CSS_OUTPUT) --minify

# Run development server
run: dev
	../../target/debug/bp-web

# Run release server
run-release: release
	../../target/release/bp-web

# Clean build artifacts
clean:
	cargo clean
	rm -f $(CSS_OUTPUT)
