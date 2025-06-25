# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

GoatNS is an authoritative DNS server written in Rust with the following key features:
- DNS over UDP/TCP on standard ports
- DNS over HTTPS (RFC8484) on `/dns-query`
- Web API and UI for zone management
- OIDC/OAuth2 authentication for the web interface
- API token authentication for management endpoints
- SQLite database backend for storing zones and records

## Development Commands

### Building and Testing
```bash
# Build release binary
cargo build --release

# Run tests
cargo test

# Run all quality checks (includes clippy, codespell, tests, doc checks)
just check

# Run individual checks
just clippy        # Lint checking
just codespell     # Spell checking
just test         # Unit tests
just doc_check    # Documentation formatting check
```

### Running the Server
```bash
# Run in development mode
just run
# or
cargo run -- server

# Build and run docker container
just docker_build
just run_container
```

### Documentation
```bash
# Build rust documentation
cargo doc --document-private-items

# Build and serve the book
just book
# or
cd docs && mdbook serve
```

### Code Quality
```bash
# Run security analysis
just semgrep

# Run coverage analysis
just coverage        # Uses tarpaulin, outputs to tarpaulin-report.html

# Format documentation
just doc_fix        # Fix markdown formatting
```

## Architecture

### Core Components

**DNS Processing Pipeline:**
- `src/main.rs` - Entry point and server orchestration
- `src/servers.rs` - UDP/TCP DNS server implementations
- `src/reply.rs` - DNS response generation
- `src/resourcerecord.rs` - DNS record handling
- `src/packet_dumper.rs` - Network packet debugging

**Data Storage:**
- `src/datastore.rs` - Main data management layer with concurrent access
- `src/db/` - Database entities and migrations using Sea-ORM
- `src/zones.rs` - Zone file parsing and management

**Web Interface:**
- `src/web/` - Axum-based HTTP server
- `src/web/api/` - REST API endpoints for zone management
- `src/web/ui/` - HTML templates and user interface
- `src/web/doh/` - DNS over HTTPS implementation
- `templates/` - Askama HTML templates

**Configuration and Utilities:**
- `src/config.rs` - Configuration file handling (JSON format)
- `src/cli.rs` - Command-line interface using clap
- `src/enums.rs` - DNS protocol enums and constants
- `src/utils.rs` - Shared utilities and channel management

### Key Libraries
- **Networking:** tokio for async I/O, axum for HTTP
- **DNS:** Custom implementation using packed_struct for protocol handling
- **Database:** Sea-ORM with SQLite backend
- **Web UI:** Askama templates with Bootstrap 5
- **Authentication:** OAuth2/OIDC support via openidconnect crate

### Testing Structure
- `src/tests/` - Integration and unit tests
- `benches/` - Performance benchmarks
- Test configuration examples in `examples/test_config/`

### Configuration
- Main config: `goatns.example.json` (example configuration)
- Zone files: JSON format (see `zones.json`, `hello.goat.json`)
- Database: SQLite with automatic migrations

## Development Notes

### Code Style
- Uses strict Clippy linting (see `clippy.toml`)
- Forbids unsafe code and unwrap/expect usage outside tests
- Requires documentation for public APIs
- Follows Rust 2024 edition standards

### Database Workflow
The application uses Sea-ORM for database operations with automatic migrations. The database schema is defined in `src/db/entities/` with models for users, zones, records, sessions, and API tokens.

### DNS Protocol Implementation
The DNS protocol implementation is custom-built using the `packed_struct` crate for efficient binary serialization. Key structures are defined in `src/lib.rs` including `Header`, `Question`, and `ResourceRecord`.

## Code Patterns and Best Practices

### General Guidelines
- Don't use `expect` unless in tests
  - Prefer proper error handling with `Result` and `?` operator
  - Handle potential failures gracefully

## Task Completion Requirements

**CRITICAL:** All tasks and sub-tasks must follow this completion workflow:

1. **Quality Gate:** Run `just check` and ensure ALL tests pass without warnings or errors
2. **Git Commit:** Create a git commit for the completed work
3. **Documentation:** Update this CLAUDE.md file if any design or implementation changes were made

**No task is considered complete until:**

- `just check` passes completely (includes clippy, codespell, tests, doc checks)
- Changes are committed to git
- CLAUDE.md is updated if architecture, design, or implementation patterns changed

**Commit Requirements:**

- Each logical change or completed task should be its own commit
- Commit messages should be clear and descriptive
- Include all related files in the commit (source code, tests, documentation)
- Never leave uncommitted changes when a task is complete

## Git Commit Guidelines

- **DO NOT mention that tests pass or that CLAUDE.md was updated in commit messages**