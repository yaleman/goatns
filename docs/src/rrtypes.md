## Supported request/record types

- [x] A (1) RFC1035
- [x] AAAA (28) RFC3596
- [ ] AFSDB (18) RFC1183
- [ ] APL (42) RFC3123 (Experimental)
- [ ] AXFR (252) RFC1035
  - [ ] add an allow-list in the config file (CIDRs)
- [x] ANY (255) RFC8482
  - [ ] should check for zones, is currently just YOLOing a response to any request
  - [x] return a HINFO record of "RFC8482", can't be stored
- [x] CAA (257) RFC6844
- [ ] CDNSKEY (60) RFC7344 Child copy of DNSKEY record, for transfer to parent
- [ ] CDNSKEY (59) RFC7344 Child copy of DS record, for transfer to parent
- [ ] CERT (37) RFC4398 Certificate record
- [ ] CSYNC (62) RFC7477 Specify a synchronization mechanism between a child and a parent DNS zone.
      Typical example is declaring the same NS records in the parent and the child zone
- [x] CNAME (5) RFC7477
- [ ] DHCID (49) RFC4701 Used in conjunction with the FQDN option to DHCP
- [ ] DLV (32769) RFC4431 DNSSEC Lookaside Validation record
- [ ] DNAME (39) RFC6672 Delegation name record
- [ ] DNSKEY (48) RFC4034 DNS Key record The key record used in DNSSEC.
- [ ] EUI48 (108) RFC7043 MAC address (EUI-48) A 48-bit IEEE Extended Unique Identifier.
- [ ] EUI64 (109) RFC7043 MAC address (EUI-64) A 64-bit IEEE Extended Unique Identifier.
- [x] HINFO (13) RFC8482 Providing Minimal-Sized Responses to DNS Queries That Have QTYPE=ANY
  - [x] RFC1035 interpretation is done
  - [x] Check up on the details in RFC8482 - "Unobsoleted by RFC 8482."
- [ ] HIP (55) RFC8005 Host Identity Protocol Method of separating the end-point identifier and
      locator roles of IP addresses.
- [ ] HTTPS (65) (IETF
      Draft)[https://datatracker.ietf.org/doc/draft-ietf-dnsop-svcb-https/00/?include_text=1] HTTPS
      Binding RR that improves performance for clients that need to resolve many resources to access
      a domain. More info in this IETF Draft by DNSOP Working group and Akamai technologies.
- [ ] IPSECKEY (45) RFC4025 IPsec Key Key record that can be used with IPsec
- [ ] KX (36) RFC2230 Key Exchanger record Used with some cryptographic systems (not including
      DNSSEC) to identify a key management agent for the associated domain-name. Note that this has
      nothing to do with DNS Security. It is Informational status, rather than being on the IETF
      standards-track. It has always had limited deployment, but is still in use.
- [x] LOC (29) RFC1876 Location record
- [x] MX (15) RFC1035 and RFC7505 Mail exchange record
- [ ] NAPTR (35) RFC3403 Naming Authority Pointer Allows regular-expression-based rewriting of
      domain names which can then be used as URIs, further domain names to lookups, etc.
- [x] NS
- [ ] NSEC (47) RFC4034 Next Secure record Part of DNSSEC—used to prove a name does not exist. Uses
      the same format as the (obsolete) NXT record.
- [ ] NSEC3 50 RFC5155 Next Secure record version 3 An extension to DNSSEC that allows proof of
      nonexistence for a name without permitting zonewalking
- [ ] NSEC3PARAM 51 RFC5155 NSEC3 parameters Parameter record for use with NSEC3
- [ ] OPENPGPKEY 61 RFC7929 OpenPGP public key record A DNS-based Authentication of Named Entities
      (DANE) method for publishing and locating OpenPGP public keys in DNS for a specific email
      address using an OPENPGPKEY DNS resource record.
- [ ] OPT 41 RFC6891 Option This is a pseudo-record type needed to support EDNS.
- [x] PTR 12 RFC1035
- [ ] RRSIG 46 RFC4034 DNSSEC signature Signature for a DNSSEC-secured record set. Uses the same
      format as the SIG record.
- [ ] RP 17 RFC1183 Responsible Person Information about the responsible person(s) for the domain.
      Usually an email address with the @ replaced by a .
- [ ] SMIMEA 53 RFC8162 S/MIME cert association Associates an S/MIME certificate with a domain name
      for sender authentication.
- [x] SOA (6) RFC1035 and RFC2308 Start of [a zone of] authority record
- [ ] SRV (33) RFC2782 Service locator
- [ ] SSHFP (44) RFC4255 SSH Public Key Fingerprint Resource record for publishing SSH public host
      key fingerprints in the DNS, in order to aid in verifying the authenticity of the host.
      RFC6594 defines ECC SSH keys and SHA-256 hashes. See the
      [IANA SSHFP RR parameters registry](https://www.iana.org/assignments/dns-sshfp-rr-parameters/dns-sshfp-rr-parameters.xml)
      for details
- [ ] SVCB 64 IETF Draft Service Binding RR that improves performance for clients that need to
      resolve many resources to access a domain. More info in this
      [IETF Draft](https://datatracker.ietf.org/doc/draft-ietf-dnsop-svcb-https/00/?include_text=1)
      by DNSOP Working group and Akamai technologies.
- [ ] TA 32768 — DNSSEC Trust Authorities Part of a deployment proposal for DNSSEC without a signed
      DNS root. See the [IANA database](https://www.iana.org/assignments/dns-parameters) and
      [Weiler Spec](http://www.watson.org/~weiler/INI1999-19.pdf) for details. Uses the same format
      as the DS record.
- [ ] TKEY 249 RFC2930 Transaction Key record A method of providing keying material to be used with
      TSIG that is encrypted under the public key in an accompanying KEY RR.
- [ ] TLSA 52 RFC6698 TLSA certificate association A record for DANE.
- [ ] TSIG 250 RFC2845 Transaction Signature Can be used to authenticate dynamic updates as coming
      from an approved client, or to authenticate responses as coming from an approved recursive
      name server
- [x] TXT 16 RFC1035 Text record
- [x] URI 256 [RFC7553](https://www.rfc-editor.org/rfc/rfc7553) Uniform Resource Identifier
- [ ] ZONEMD (63) RFC8976 Message Digests for DNS Zones Provides a cryptographic message digest over
      DNS zone data at rest.

A lot of the details above were transcribed from the
[Wikipedia page on DNS REcord Types](https://en.wikipedia.org/wiki/List_of_DNS_record_types)
