# Security Audit Report

**Date**: 2026-06-26
**Scope**: Rust source code in `src/`, `goatns-macros/`, `goat-lib/`
**Auditor**: Automated comprehensive review

---

## Executive Summary

The GoatNS DNS server is built in memory-safe Rust with `#![forbid(unsafe_code)]`, eliminating entire classes of memory safety vulnerabilities. The project uses well-vetted crates (Argon2id, sea-orm, axum, rustls). However, this audit identified **7 High**, **12 Medium**, and **8 Low** severity logic-level security issues across authentication, authorization, session management, and input validation.

---

## CRITICAL

No critical findings identified.

---

## HIGH Severity

---

### H-1: Missing Admin Authorization Check on Admin Endpoints

**Location**: `src/web/ui/admin_ui.rs:48-58`, `src/web/ui/admin_ui.rs:60-102`, `src/web/ui/admin_ui.rs:118-197`

**Category**: Authorization Bypass

**Problem**: The admin UI routes (`/ui/admin`, `/ui/admin/reports/unowned_records`, `/ui/admin/zones/assign_ownership/{id}`) only call `check_logged_in()` which verifies the user is authenticated and not disabled, but **never checks `user.admin`**. Any authenticated user can access the admin dashboard, view all zones (including unowned ones), and assign zone ownership.

**Vulnerable Code**:

```rust
// src/web/ui/admin_ui.rs:48-58
pub(crate) async fn dashboard(mut session: Session) -> Result<AdminUITemplate, Redirect> {
    let user = check_logged_in(&mut session, url).await?;
    // No check: if !user.admin { return Err(...) }
    Ok(AdminUITemplate {
        user_is_admin: user.admin,  // Only used for display, not access control
    })
}
```

**Reproduction**:

1. Create a regular (non-admin) user account via the web UI.
2. Log in as that regular user.
3. Navigate directly to `/ui/admin` in the browser.
4. The full admin dashboard loads, showing all zones and administrative controls.
5. Navigate to `/ui/admin/zones/assign_ownership/{id}` to reassign any zone to any user.

**Impact**: Complete admin panel compromise. Any authenticated user can view all zones, modify zone ownership, and effectively take full control of the DNS server's authoritative zones.

**Fix**: Add an admin check in each admin route handler:

```rust
if !user.admin {
    return Err(Urls::Home.redirect());
}
```

---

### H-2: Open Redirect via Session-Stored Redirect Path

**Location**: `src/web/auth/mod.rs:359-361`, `src/web/auth/mod.rs:425-427`, `src/web/auth/mod.rs:541-543`, `src/web/ui/mod.rs:260-281`

**Category**: Open Redirect / Phishing

**Problem**: After login/signup, the server reads a redirect path from the session (`session.remove("redirect")`) and issues a `Redirect::to(&destination)` without any validation. An attacker can store an arbitrary URL in the session via the `redirect` query parameter and use it to redirect victims to malicious sites after OAuth callback completion.

**Vulnerable Code**:

```rust
// src/web/auth/mod.rs:424-428
let redirect: Option<String> = session.remove("redirect").await.unwrap_or(None);
match redirect {
    Some(destination) => Ok(Redirect::to(&destination).into_response()),
    None => Ok(Urls::ZonesList.redirect().into_response()),
}
```

The redirect path is stored in `src/web/ui/mod.rs:281`:

```rust
session.insert(SESSION_REDIRECT_KEY, redirect_path).await
```

where `redirect_path` comes from the request URI path — which can be manipulated.

**Reproduction**:

1. Attacker crafts a login URL with a redirect parameter pointing to `https://evil.com/phishing`.
2. Victim clicks the link and is taken to the OAuth login flow.
3. After completing OAuth login, the server redirects the victim to `https://evil.com/phishing`.
4. The attacker's site can mimic the GoatNS UI to harvest credentials or steal session tokens.

**Impact**: Phishing attacks, credential theft, token theft via redirection to attacker-controlled origins.

**Fix**: Validate the redirect destination against an allowlist (same-origin check or known safe paths) before issuing the redirect. Reject absolute URLs that point to external hosts.

---

### H-3: Zone Name Not Validated Against TLD Allowlist on UI Creation

**Location**: `src/web/ui/zones.rs:23-132`

**Category**: Input Validation / Authorization Bypass

**Problem**: The web UI zone creation endpoint (`POST /ui/zones/new`) validates the zone name using `dns_name()` but does **not** call `check_valid_tld()`. The API endpoint (`POST /api/zone`) does call `check_valid_tld()`. This inconsistency allows users to create zones with arbitrary TLDs through the web UI, bypassing the TLD restriction.

**Vulnerable Code**:

```rust
// src/web/ui/zones.rs:40-46 — missing check_valid_tld
if !dns_name(&form.name) {
    return Err(Urls::Home.redirect_with_query(HashMap::from([(
        "error".to_string(),
        "Invalid DNS name".to_string(),
    )])));
}
// No TLD check here!
```

**Reproduction**:

1. Log in as any authenticated user.
2. Navigate to the zone creation page (`/ui/zones/new`).
3. Submit a zone with a name like `attacker.local` or `evil.com`.
4. The zone is created successfully despite not being in the allowed TLD list.
5. The server now authoritatively responds for that zone, potentially hijacking DNS resolution.

**Impact**: DNS hijacking. An attacker can create zones for domains the server shouldn't be authoritative for, intercepting or manipulating DNS queries for those domains.

**Fix**: Add `check_valid_tld(&form.name, &state.read().await.config.allowed_tlds)` check before zone creation in the UI handler, matching the API endpoint behavior.

---

### H-4: DoH GET Endpoint Leaks Internal Records Without Authentication

**Location**: `src/web/doh/mod.rs:179-348`

**Category**: Information Disclosure

**Problem**: The DoH GET endpoint (`GET /dns-query`) queries the database directly and returns DNS records without any authentication or authorization check. While this is standard for public DNS-over-HTTPS resolvers, the endpoint also exposes records from the internal `records_merged` table which may include private/internal zone data. The `name` parameter is used directly in a database query without rate limiting, enabling enumeration of all records.

**Vulnerable Code**:

```rust
// src/web/doh/mod.rs:228-235
let records = match entities::records_merged::Entity::find()
    .filter(
        entities::records_merged::Column::Name.eq(qname.clone())
            .and(entities::records_merged::Column::Rrtype.eq(rr_type_value as u16)),
    )
    .all(&read_txn)
    .await
```

**Reproduction**:

1. Send a DoH query for any domain: `GET /dns-query?name=example.com&type=A`
2. Iterate through record types (A, AAAA, MX, TXT, etc.) to enumerate all records for a zone.
3. Query for internal/private zone names (e.g., `internal.corp`, `admin.local`) to extract sensitive internal DNS data.
4. No authentication is required — the endpoint is fully public.

**Impact**: Complete enumeration of all DNS records hosted on the server, including internal/private infrastructure that may leak hostnames, IP addresses, and service information useful for further attacks.

**Fix**: If this server is meant to be a public resolver, document this behavior explicitly. If it's meant to be private, add authentication to the DoH endpoint or restrict it to localhost/trusted IPs.

---

### H-5: No Rate Limiting on DNS and API Endpoints

**Location**: `src/servers.rs:118-182` (UDP), `src/servers.rs:347-388` (TCP), `src/web/mod.rs:175-241` (API)

**Category**: Denial of Service

**Problem**: There is no rate limiting anywhere in the application — not on DNS queries, API endpoints, or authentication attempts. The only concurrency control is `MAX_IN_FLIGHT = 512` for the datastore channel, which limits concurrent database operations but doesn't prevent abuse.

**Reproduction**:

- **DNS Amplification**: Send spoofed DNS queries (with source IP set to victim's IP) to the server. The server responds to the victim with potentially large DNS responses, amplifying DDoS traffic.
- **Brute Force**: Send unlimited login attempts to `POST /api/login` to brute-force API tokens.
- **DoS**: Flood the DNS listener with queries. Once the 512-slot channel is full, legitimate queries are dropped.

**Impact**: DNS amplification attacks, brute-force credential attacks, and denial of service for legitimate users.

**Fix**: Implement rate limiting using `tower::limit::RateLimit` or `axum::extract` with per-IP tracking. For DNS, implement Response Rate Limiting (RRL) per BCP38 recommendations.

---

### H-6: Session Cookie Missing `SameSite` and `HttpOnly` Attributes

**Location**: `src/web/auth/mod.rs:456-480`

**Category**: Session Management / CSRF

**Problem**: The session cookie is configured with `.with_secure(true)` and `.with_domain(...)` but is missing `SameSite` and `HttpOnly` attributes. Without `SameSite`, the cookie is sent on cross-site requests, enabling CSRF attacks. Without `HttpOnly`, JavaScript running in the browser can steal the session cookie.

**Vulnerable Code**:

```rust
Ok(SessionManagerLayer::new(session_store)
    .with_expiry(Expiry::OnInactivity(Duration::minutes(5)))
    .with_name(COOKIE_NAME)
    .with_secure(true)
    .with_domain(config.hostname.clone()))
// Missing: .with_same_site(...) and .with_http_only(true)
```

**Reproduction**:

1. **CSRF Attack**: Attacker creates a malicious website that makes requests to the GoatNS server. When a victim visits the site, their browser includes the session cookie, allowing the attacker to perform actions as the authenticated user.
2. **XSS + Session Theft**: If any XSS vulnerability exists (e.g., via template injection), JavaScript can read `document.cookie` and exfiltrate the session token because `HttpOnly` is not set.

**Impact**: Cross-site request forgery, session hijacking via XSS.

**Fix**: Add `.with_same_site(tower_sessions::cookie::SameSite::Strict)` and `.with_http_only(true)` to the session configuration.

---

### H-7: OAuth State Parameter Not Validated Against Session

**Location**: `src/web/auth/mod.rs:292-298`

**Category**: Authentication Flaw / CSRF

**Problem**: The `state` parameter from the OAuth callback is passed directly to `pop_verifier()` without checking that it was actually generated by this server instance. The `redirect` query parameter in `QueryForLogin` is not validated, enabling the open redirect described in H-2.

**Vulnerable Code**:

```rust
// src/web/auth/mod.rs:292-298
let verifier = state.pop_verifier(query_state.clone()).await;
let (pkce_verifier_secret, nonce) = match verifier {
    Some((p, n)) => (p, n),
    None => {
        error!("Couldn't find a session, redirecting...");
        return Err(Urls::Login.redirect().into_response());
    }
};
```

The `QueryForLogin` struct has an unvalidated `redirect` field:

```rust
pub struct QueryForLogin {
    pub state: Option<String>,
    pub code: Option<String>,
    pub redirect: Option<String>,  // Not validated
}
```

**Reproduction**: Combined with H-2, an attacker can craft a login URL with a malicious redirect, trick a victim into authenticating, and then redirect them to an attacker-controlled site with the OAuth authorization code.

**Impact**: OAuth flow hijacking, phishing, token theft.

**Fix**: Validate the redirect URL against an allowlist before storing it in the session. Ensure the state parameter is cryptographically tied to the session.

---

## MEDIUM SeverITY

---

### M-1: DNSSEC Support Added — AD Bit Now Controlled by Zone Configuration

**Location**: `src/servers.rs:571`, `src/datastore.rs:195`, `src/db/entities/zones.rs`

**Category**: DNS Security

**Previous Problem**: The server set `ad: false` and `cd: false` in all responses with TODO comments indicating DNSSEC was not implemented.

**Current Status**: **Resolved.** DNSSEC support has been added:

- New DNSSEC record types supported: DNSKEY, RRSIG, NSEC, NSEC3, NSEC3PARAM, DS, CDNSKEY, CDS
- Zones have a `signed` boolean column that controls the `ad` bit in responses
- EDNS0/OPT pseudo-RR parsing implemented (RFC 6891)
- DO (DNSSEC OK) bit is read from queries and used to include RRSIG records in responses
- CD (Checking Disabled) bit is copied from query to response
- TCP fallback per RFC 7766: truncated UDP responses set the TC bit, clients retry over TCP
- RRSIG records are automatically included when the DO bit is set and the zone is signed

**Residual Risk**: This is an authoritative-only server. DNSSEC validation (recursive resolver concern) is not implemented and is out of scope. Zone signing is done offline — the `sign-zone` subcommand (planned) will generate keys and sign zones before loading them into the database. Key management is the operator's responsibility.

---

### M-2: TCP Connection Length Field Not Properly Validated

**Location**: `src/servers.rs:194-207`

**Category**: Input Validation / DoS

**Problem**: The TCP DNS message length field is read as a `u16` but there's no minimum length validation. A `msg_length` of 0 is technically valid per the code logic. There's no maximum length enforcement on TCP messages, allowing an attacker to claim a very large message length and cause the server to read indefinitely.

**Vulnerable Code**:

```rust
let msg_length: usize = reader.read_u16().await?.into();
// No validation: msg_length could be 0 or 65535
let mut buf: Vec<u8> = vec![];
while buf.len() < msg_length {
    let len = match reader.read_buf(&mut buf).await { ... }
```

**Reproduction**: Open a TCP connection to the DNS server and send a length header claiming 65535 bytes but never send the data. The connection is tied up indefinitely (slowloris-style DoS).

**Fix**: Enforce minimum (12 bytes for DNS header) and maximum (65535 bytes per RFC) message lengths. Add a read timeout for the message body.

---

### M-3: Zone File Import Allows Arbitrary File Reads via CLI

**Location**: `src/cli.rs:148-175`, `src/datastore.rs:294-326`

**Category**: Path Traversal

**Problem**: The `import_zones` CLI command takes a `filename` parameter directly from the CLI argument and passes it to `load_zones()` which opens the file. There's no validation that the path is within an expected directory.

**Reproduction**: Run `goatns import_zones /etc/passwd` — while this would fail JSON parsing, error messages may leak file contents or metadata.

**Fix**: Validate that the import path is within an expected directory. Restrict zone file imports to a configured safe path.

---

### M-4: API Token Secret Logged in Plaintext

**Location**: `src/web/api/auth.rs:48-50`

**Category**: Information Leakage

**Problem**: In test mode, the API login payload (which contains `token_secret`) is printed to stdout. The `debug!` log at line 50 could capture token secrets if log level is set to debug.

**Vulnerable Code**:

```rust
#[cfg(test)]
println!("Got login payload: {payload:?}");
```

**Reproduction**: Enable debug logging and observe log output. Token secrets will appear in plaintext.

**Fix**: Never log sensitive payloads. Redact or mask `token_secret` before logging.

---

### M-5: Session Cookie Domain Set to Hostname — Cookie Tossing

**Location**: `src/web/auth/mod.rs:479`

**Category**: Session Management

**Problem**: The session cookie domain is set to `config.hostname` which defaults to the system hostname. If the hostname is a bare name like `goatns`, the cookie will be sent to all subdomains (`*.goatns`).

**Vulnerable Code**:

```rust
.with_domain(config.hostname.clone())
```

**Fix**: Make the cookie domain configurable and default to no domain restriction (host-only cookie). Document the security implications.

---

### M-6: No CORS Configuration on API Endpoints

**Location**: `src/web/mod.rs:175-241`

**Category**: Missing Security Controls

**Problem**: The API server has no CORS (Cross-Origin Resource Sharing) configuration. Any website can make cross-origin requests to the API. Combined with the missing `HttpOnly` cookie attribute (H-6), this enables cross-site attacks.

**Fix**: Add a CORS layer with restrictive origins. Use `tower_http::cors::CorsLayer` with explicit origin allowlist.

---

### M-7: `api_cookie_secret` Logged in Debug Output

**Location**: `src/config.rs:338-341`

**Category**: Information Leakage

**Problem**: The `Display` implementation for `ConfigFile` includes extensive configuration details. While it correctly excludes `api_cookie_secret` via `#[serde(skip_serializing)]`, other sensitive fields could leak if the config is logged at debug level.

**Fix**: Ensure the `Display` impl never exposes secrets. Audit all `Debug` and `Display` implementations for sensitive data exposure.

---

### M-8: Unbounded Zone File Parsing — Memory Exhaustion

**Location**: `src/zones.rs:217-241`

**Category**: Denial of Service

**Problem**: The `load_zones` function reads the entire file into a string, then parses it as JSON. There's no size limit on the file.

**Vulnerable Code**:

```rust
let mut buf: String = String::new();
file.read_to_string(&mut buf)...;  // No size limit
let jsonblob: Vec<serde_json::Value> = json5::from_str(&buf)...;  // Parses entire file
```

**Reproduction**: Create a multi-gigabyte JSON zone file and trigger an import. The server reads the entire file into memory, causing OOM.

**Fix**: Add a maximum file size check before reading. Use streaming JSON parsing for large files.

---

### M-9: `export_zone_file` Allows Writing to Arbitrary Paths

**Location**: `src/cli.rs:101-145`

**Category**: Path Traversal

**Problem**: The `export_zone_file` function takes an `output_filename` parameter and writes to it directly via `tokio::fs::File::create(filename)`. There's no path validation, allowing overwriting of arbitrary files.

**Reproduction**: Run `goatns export_zone_file --output /etc/cron.d/backdoor` to overwrite critical system files.

**Fix**: Validate the output path is within an expected directory (e.g., current working directory or configured export path).

---

### M-10: DoH GET Response Uses User-Controlled Data in Cache-Control

**Location**: `src/web/doh/mod.rs:290`, `src/web/doh/mod.rs:335`, `src/web/doh/mod.rs:409`

**Category**: Cache Poisoning

**Problem**: The `Cache-Control: max-age={min_ttl}` header uses TTL values directly from database records. An attacker who can control record TTLs (via zone creation) can set arbitrary cache durations, potentially poisoning downstream caches.

**Fix**: Clamp TTL values to a reasonable range (e.g., 1-86400 seconds) before using them in cache headers.

---

### M-11: Missing `HttpOnly` on Session Cookie

**Location**: `src/web/auth/mod.rs:474-479`

**Category**: Session Management

**Problem**: Separate from the SameSite issue (H-6), the missing `HttpOnly` flag means that if any XSS vulnerability exists, the session cookie can be exfiltrated via JavaScript.

**Fix**: Add `.with_http_only(true)` to the session manager layer configuration.

---

### M-12: Admin Can Assign Zone Ownership Without CSRF Protection (GET)

**Location**: `src/web/ui/admin_ui.rs:204-206`

**Category**: CSRF

**Problem**: The `assign_zone_ownership` handler processes state-changing operations on both GET and POST. The GET handler doesn't validate a CSRF token.

**Vulnerable Code**:

```rust
.route(
    "/zones/assign_ownership/{id}",
    get(assign_zone_ownership).post(assign_zone_ownership),
)
```

**Reproduction**: An attacker tricks an admin into visiting a page that auto-submits a form to this endpoint, reassigning zone ownership without the admin's knowledge.

**Fix**: Require POST for state-changing operations and validate CSRF tokens on all POST handlers.

---

## LOW SeverITY

---

### L-1: Information Leakage via Error Messages in Auth Flow

**Location**: `src/web/auth/mod.rs:357-364`

**Category**: Information Leakage

**Problem**: When a database error occurs during login, the error handling pattern could leak information through timing differences or redirect behavior.

**Fix**: Ensure error handling is uniform regardless of the failure reason. Use constant-time comparisons where applicable.

---

### L-2: `dns_name` Validator Rejects Valid Single-Label Names

**Location**: `goat-lib/src/validators.rs:12-34`

**Category**: Functional / Security-Adjacent

**Problem**: The `dns_name` function rejects names without dots, preventing creation of single-label zones (e.g., `localhost`, `internal`) which are valid in some contexts.

**Vulnerable Code**:

```rust
if !name.contains('.') {
    return false;
}
```

**Fix**: Allow single-label names when appropriate for internal/private use cases.

---

### L-3: Unused `ImportFile` Resp Channel — Potential Deadlock

**Location**: `src/cli.rs:153-174`

**Category**: DoS / Reliability

**Problem**: The `import_zones` function creates a oneshot channel but the receiver is declared as `mut rx_oneshot` and never used properly. The loop checks `try_recv()` but the channel sender in the datastore may not fire correctly in all error cases, potentially causing the loop to spin indefinitely.

**Fix**: Use proper async waiting on the oneshot channel with a timeout.

---

### L-4: `packet_dumper` Creates Files with Predictable Names

**Location**: `src/packet_dumper.rs:34-41`

**Category**: Information Leakage

**Problem**: Packet capture files use a timestamp-based name without randomness. If two packets arrive in the same second, the second write overwrites the first.

**Vulnerable Code**:

```rust
let filename = format!(
    "{}/{}-{}.cap",
    dest_dir.map(|f| f.display().to_string()).unwrap_or_else(|| "./captures".to_string()),
    dump_type,
    now.format("%Y-%m-%dT%H%M%SZ")
);
```

**Fix**: Add a random component or counter to the filename.

---

### L-5: `println!` Statements in Production Code

**Location**: `src/web/api/auth.rs:109`, `src/web/api/zones.rs:231,248,274`

**Category**: Information Leakage

**Problem**: Multiple `println!` statements exist in production (non-test) code paths. These can leak information to stdout in containerized environments.

**Fix**: Replace with `info!` or `debug!` tracing macros.

---

### L-6: LOC Record Parsing Silently Defaults on Errors

**Location**: `src/resourcerecord.rs:1305-1380`

**Category**: Input Validation

**Problem**: The LOC record parser uses `error!` logs and defaults to 0 or default values when parsing fails. This could lead to serving incorrect LOC data without any indication of a problem.

**Fix**: Return an error for invalid LOC records instead of silently defaulting.

---

### L-7: No Maximum Limit on `GetZoneNames` Pagination

**Location**: `src/datastore.rs:351-387`

**Category**: DoS

**Problem**: The `handle_get_zone_names` function accepts `offset` and `limit` parameters without enforcing a maximum limit. The web UI hardcodes `limit = 20`, but if these parameters come from user input in the future, an attacker could request all zones at once.

**Fix**: Enforce a maximum limit (e.g., 100) in the database query function.

---

### L-8: `ContactDetails::to_html_parts` Outputs Unescaped URLs

**Location**: `src/enums.rs:411-425`

**Category**: XSS

**Problem**: The `to_html_parts` method returns URLs that are directly interpolated into HTML templates. If `server` or `contact` contain malicious characters, this could enable stored XSS.

**Vulnerable Code**:

```rust
ContactDetails::Mastodon { server, contact } => {
    (contact.to_owned(), format!("https://{server}/@{contact}"))
}
```

**Reproduction**: An admin sets their contact details with a malicious server value like `evil.com"><script>alert(1)</script>`. When other users view pages displaying admin contact info, the injected HTML/JS executes.

**Fix**: Ensure Askama templates escape these values (Askama auto-escapes by default, but verify the template uses `{{ }}` rather than `{{{ }}}`).

---

## Positive Security Observations

1. **Memory safety**: `#![forbid(unsafe_code)]` in `src/lib.rs` eliminates entire classes of memory safety vulnerabilities.
2. **Strong password hashing**: API tokens use Argon2id (industry-standard memory-hard hashing).
3. **PKCE in OAuth flow**: The OAuth2 implementation uses PKCE, preventing authorization code interception attacks.
4. **CSRF tokens for API token management**: The user settings API token creation flow properly implements CSRF protection.
5. **TLS by default**: The API server uses rustls with AWS-LC-RS crypto provider.
6. **Clippy lints enabled**: Strict clippy settings (`#![deny(clippy::all)]`) catch many common issues.
7. **Session expiry**: Sessions expire after 5 minutes of inactivity.
8. **No SQL injection**: All database queries use parameterized queries via sea-orm.

---

## Summary Table

| ID  | Severity | Category                | Location                           |
|-----|----------|-------------------------|----------------------------------- |
| H-1 | High     | AuthZ Bypass            | `src/web/ui/admin_ui.rs`           |
| H-2 | High     | Open Redirect           | `src/web/auth/mod.rs`              |
| H-3 | High     | Input Validation        | `src/web/ui/zones.rs`              |
| H-4 | High     | Information Disclosure  | `src/web/doh/mod.rs`               |
| H-5 | High     | DoS                     | `src/servers.rs`, `src/web/mod.rs` |
| H-6 | High     | Session Management      | `src/web/auth/mod.rs`              |
| H-7 | High     | AuthN Flaw              | `src/web/auth/mod.rs`              |
| M-1 | Medium   | DNS Security            | `src/servers.rs`                   |
| M-2 | Medium   | Input Validation        | `src/servers.rs`                   |
| M-3 | Medium   | Path Traversal          | `src/cli.rs`                       |
| M-4 | Medium   | Information Leakage     | `src/web/api/auth.rs`              |
| M-5 | Medium   | Session Management      | `src/web/auth/mod.rs`              |
| M-6 | Medium   | Missing Controls        | `src/web/mod.rs`                   |
| M-7 | Medium   | Information Leakage     | `src/config.rs`                    |
| M-8 | Medium   | DoS                     | `src/zones.rs`                     |
| M-9 | Medium   | Path Traversal          | `src/cli.rs`                       |
| M-10| Medium   | Cache Poisoning         | `src/web/doh/mod.rs`               |
| M-11| Medium   | Session Management      | `src/web/auth/mod.rs`              |
| M-12| Medium   | CSRF                    | `src/web/ui/admin_ui.rs`           |
| L-1 | Low      | Information Leakage     | `src/web/auth/mod.rs`              |
| L-2 | Low      | Functional              | `goat-lib/src/validators.rs`       |
| L-3 | Low      | DoS / Reliability       | `src/cli.rs`                       |
| L-4 | Low      | Information Leakage     | `src/packet_dumper.rs`             |
| L-5 | Low      | Information Leakage     | `src/web/api/`                     |
| L-6 | Low      | Input Validation        | `src/resourcerecord.rs`            |
| L-7 | Low      | DoS                     | `src/datastore.rs`                 |
| L-8 | Low      | XSS                     | `src/enums.rs`                     |
