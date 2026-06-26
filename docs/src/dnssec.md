# DNSSEC Support

GoatNS supports serving DNSSEC-signed zones. This is an authoritative-only server — it does not perform DNSSEC validation (that is a recursive resolver concern). The deployment model is **offline signing**: zones are signed by a companion tool before being loaded into the server.

## Overview

DNSSEC (Domain Name System Security Extensions) provides authentication and integrity for DNS data using public-key cryptography. When a zone is signed:

1. Each RRset (group of records with the same name and type) gets an RRSIG (signature) record
2. A chain of NSEC or NSEC3 records provides authenticated denial of existence
3. DNSKEY records publish the public keys used for verification
4. DS records in the parent zone establish the chain of trust

## Supported Record Types

| Type | Code | RFC | Purpose |
|------|------|-----|---------|
| DNSKEY | 48 | RFC 4034 | Public key used in DNSSEC |
| RRSIG | 46 | RFC 4034 | Signature for an RRset |
| NSEC | 47 | RFC 4034 | Next Secure record (authenticated denial) |
| NSEC3 | 50 | RFC 5155 | Next Secure v3 (hashed denial) |
| NSEC3PARAM | 51 | RFC 5155 | NSEC3 parameters for the zone |
| DS | 43 | RFC 4034 | Delegation Signer (parent zone) |
| CDNSKEY | 60 | RFC 7344 | Child DNSKEY (for CDS) |
| CDS | 59 | RFC 7344 | Child DS (for parent zone updates) |

## Zone Signing Workflow

### 1. Generate Keys

Generate a Key Signing Key (KSK) and Zone Signing Key (ZSK):

```bash
goatns sign-zone generate-keys --zone example.com --output-dir /path/to/keys/
```

This creates:
- `example.com.ksk.pem` — KSK private key
- `example.com.zsk.pem` — ZSK private key

### 2. Sign the Zone

```bash
goatns sign-zone sign \
  --zone example.com \
  --ksk /path/to/keys/example.com.ksk.pem \
  --zsk /path/to/keys/example.com.zsk.pem \
  --input-zone zones.json \
  --output-zone signed_zones.json
```

This adds DNSKEY, RRSIG, NSEC/NSEC3, and DS records to the zone.

### 3. Import the Signed Zone

```bash
goatns import-zones --file signed_zones.json
```

The zone's `signed` flag will be set to `true` automatically when DNSKEY records are present.

### 4. Publish DS Records

Upload the DS records to your registrar or parent zone operator. This establishes the chain of trust.

## Server Behavior

### AD Bit

The `ad` (Authentic Data) bit in the response header is set to `true` when:
- The zone has `signed: true` in the database
- The response contains valid RRSIG records

### DO Bit

When a client sets the DO (DNSSEC OK) bit in an OPT pseudo-RR:
- RRSIG records matching the queried type are included in the answer section
- The response includes an OPT RR in the additional section

### EDNS0/OPT

The server supports EDNS0 (RFC 6891):
- Parses the OPT pseudo-RR from the query's additional section
- Includes an OPT RR in the response when the query had one
- UDP payload size is set to 1232 bytes (modern minimum)

### TCP Fallback

Per RFC 7766:
- If a UDP response exceeds the UDP buffer size, the TC (Truncated) bit is set
- The client should retry over TCP to get the full response
- TCP responses are not truncated

## Configuration

DNSSEC is configured per-zone via the `signed` flag:

```json
{
  "name": "example.com",
  "rname": "admin@example.com",
  "serial": 2024010101,
  "refresh": 3600,
  "retry": 600,
  "expire": 604800,
  "minimum": 86400,
  "signed": true
}
```

## Key Rotation

Key rotation is performed offline:

1. Generate new key(s)
2. Sign the zone with both old and new keys during the rollover period
3. Wait for the old signatures to expire from caches
4. Remove the old key and re-sign with only the new key
5. Update DS records in the parent zone if the KSK changed

## Security Considerations

- Private keys are never stored on the running server
- The server does not perform cryptographic operations during query handling
- Zone signing is done offline, keeping keys away from the network
- The `signed` flag must be explicitly set by the operator after verifying signatures
