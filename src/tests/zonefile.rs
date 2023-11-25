use crate::enums::RecordClass;
use crate::zonefile::*;

#[test]
fn test_parse_example_file() {
    if flexi_logger::Logger::try_with_str("debug")
        .unwrap()
        .start()
        .is_err()
    {
        eprintln!("Oh no, no logging for you!")
    };

    // example from https://access.redhat.com/documentation/en-us/red_hat_enterprise_linux/4/html/reference_guide/s2-bind-zone-examples
    let example_file = r#"$ORIGIN example.com.
    $TTL 86400
    @	IN	SOA	dns1.example.com.	hostmaster.example.com. (
                2001062501 ; serial
                21600      ; refresh after 6 hours
                3600       ; retry after 1 hour
                604800     ; expire after 1 week
                86400 )    ; minimum TTL of 1 day


        IN	NS	dns1.example.com.
        IN	NS	dns2.example.com.


        IN	MX	10	mail.example.com.
        IN	MX	20	mail2.example.com.


    dns1	IN	A	10.0.1.1
    dns2	IN	A	10.0.1.2


    server1	IN	A	10.0.1.5
    server2	IN	A	10.0.1.6


    ftp	IN	A	10.0.1.3
        IN	A	10.0.1.4

    mail	IN	CNAME	server1
    mail2	IN	CNAME	server2


    www	IN	CNAME	server1"#;
    // let lex = ZoneFileToken::lexer(&example_file);
    let res: ParsedZoneFile = parse_file(&example_file).expect("Failed at parsing stage");

    if let Some(record) = res.soarecord {
        assert_eq!(record.class, RecordClass::Internet);
        // assert_eq!(minimum, Some(86400));
    } else {
        panic!("didn't get an SOA record!");
    };
}

#[test]
fn test_parse_yaleman_file() {
    if flexi_logger::Logger::try_with_str("debug")
        .unwrap()
        .start()
        .is_err()
    {
        println!("well, no logging for you!");
    };

    let example_file = r#";;
    ;; Domain:     example.com.
    ;; Exported:   2023-11-21 13:59:08
    ;;
    ;; This file is intended for use for informational and archival
    ;; purposes ONLY and MUST be edited before use on a production
    ;; DNS server.  In particular, you must:
    ;;   -- update the SOA record with the correct authoritative name server
    ;;   -- update the SOA record with the contact e-mail address information
    ;;   -- update the NS record(s) with the authoritative name servers for this domain.
    ;;
    ;; For further information, please consult the BIND documentation
    ;; located on the following website:
    ;;
    ;; http://www.isc.org/
    ;;
    ;; And RFC 1035:
    ;;
    ;; http://www.ietf.org/rfc/rfc1035.txt
    ;;
    ;; Please note that we do NOT offer technical support for any use
    ;; of this zone data, the BIND name server, or any other third-party
    ;; DNS software.
    ;;
    ;; Use at your own risk.
    ;; SOA Record
    example.com	3600	IN	SOA	hera.ns.cloudflare.com. dns.cloudflare.com. 2045224174 10000 2400 604800 3600

    ;; NS Records
    example.com.	86400	IN	NS	hera.ns.cloudflare.com.
    example.com.	86400	IN	NS	jobs.ns.cloudflare.com.

    ;; A Records
    apache.subdomain.example.com.	1	IN	A	192.168.5.21
    auth.subdomain.example.com.	1	IN	A	192.168.5.11
    network2.example.com.	1	IN	A	8.7.6.5
    cisco.subdomain.example.com.	1	IN	A	192.168.5.8
    gateway.subdomain.example.com.	1	IN	A	192.168.5.1
    subdomain.example.com.	1	IN	A	111.222.33.44
    donkey1.subdomain.example.com.	1	IN	A	192.168.5.94
    donkey2.subdomain.example.com.	1	IN	A	192.168.5.95
    k8s.subdomain.example.com.	1	IN	A	192.168.5.91
    k8s.subdomain.example.com.	1	IN	A	192.168.5.92
    k8s.subdomain.example.com.	1	IN	A	192.168.5.94
    servce2.subdomain.example.com.	1	IN	A	192.168.5.15
    servce3.subdomain.example.com.	1	IN	A	192.168.5.23
    localhost.subdomain.example.com.	1	IN	A	127.0.0.1
    nagios.subdomain.example.com.	1	IN	A	192.168.5.19
    proxy.subdomain.example.com.	1	IN	A	192.168.5.27
    squid.subdomain.example.com.	1	IN	A	192.168.5.27
    syslog.subdomain.example.com.	1	IN	A	192.168.5.18
    unifi.subdomain.example.com.	1	IN	A	192.168.5.85
    wireguard.subdomain.example.com.	1	IN	A	192.168.5.16

    ;; AAAA Records
    donkey1.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:8888:3bff:fe04:8888
    servce2.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:24ad:bfff:fe87:d39d
    servce3.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:542c:5ff:fe5e:ab16
    microserver.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:2:4:ff:9999
    monitoring.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:f006:7777:ff:8888
    plex.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:2:1:fee7:8242
    proxy.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:fc8c:dddd:ffff:cccc
    squid.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:fc8c:dddd:ffff:cccc
    unifi.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:8462:49ff:aaaa:bbbb
    wireguard.subdomain.example.com.	1	IN	AAAA	2001:1234:be:ef:eeba::9:c:4
    wordpress.example.com.	1	IN	AAAA	2001:ffff:caf:ebee:f:1d13:6666

    ;; CAA Records
    example.com.	1	IN	CAA	0 issue "amazon.com"
    example.com.	1	IN	CAA	0 issue "letsencrypt.org"

    ;; CNAME Records
    _19b2a953166666d55e4f405f20bbbbbb.wiki.example.com.	300	IN	CNAME	_dddd34a9366655555555559bc2baaaaa.bbbbzzzzwj.acm-validations.aws.
    api.subdomain.example.com.	300	IN	CNAME	k8s.subdomain.example.com.
    backupserver.subdomain.example.com.	300	IN	CNAME	purple1.subdomain.example.com.
    fm1._domainkey.example.com.	300	IN	CNAME	fm1.example.com.dkim.fmhosted.com.
    fm2._domainkey.example.com.	300	IN	CNAME	fm2.example.com.dkim.fmhosted.com.
    fm3._domainkey.example.com.	300	IN	CNAME	fm3.example.com.dkim.fmhosted.com.
    goatns.subdomain.example.com.	60	IN	CNAME	k8s.subdomain.example.com.
    goatns.example.com.	1	IN	CNAME	21854771-64a6-4a81-b57e-2222bfeb7777.cfargotunnel.com.
    internal.servce.example.com.	300	IN	CNAME	servce2.subdomain.example.com.
    servce.example.com.	1	IN	CNAME	21854771-64a6-4a81-b57e-2222bfeb7777.cfargotunnel.com.
    left.test-servce.example.com.	300	IN	CNAME	servce3.subdomain.example.com.
    minio.example.com.	1	IN	CNAME	a-64a6-4a81-b57e-2222bfeb7777.cfargotunnel.com.
    mqtt.example.com.	1	IN	CNAME	21854771-64a6-4a81-b57e-2222bfeb7777.cfargotunnel.com.
    nagios.example.com.	1	IN	CNAME	21854771-64a6-4a81-b57e-2222bfeb7777.cfargotunnel.com.
    ntp.subdomain.example.com.	300	IN	CNAME	ntp2.subdomain.example.com.
    right.test-servce.example.com.	300	IN	CNAME	servce3.subdomain.example.com.
    saml.subdomain.example.com.	300	IN	CNAME	k8s.subdomain.example.com.
    splunk.example.com.	1	IN	CNAME	21854771-64a6-4a81-b57e-2222bfeb7777.cfargotunnel.com.
    test-servce.example.com.	300	IN	CNAME	servce3.subdomain.example.com.
    test-oauth2.example.com.	1	IN	CNAME	5852ef56-4ccb-4206-ac4a-0b6e2b43071e.cfargotunnel.com.
    wiki.example.com.	300	IN	CNAME	ohnoyoudidnt.cloudfront.net.
    www.example.com.	1	IN	CNAME	example.com.
    example.com.	300	IN	CNAME	example.github.io.

    ;; LOC Records
    pizza.example.com.	69	IN	LOC	01 02 3.000 N 01 02 3.000 E 10m 10m 10m 10m
    example.com.	1	IN	LOC	01 02 3.000 N 01 02 3.000 E 10m 10m 10m 10m

    ;; MX Records
    subdomain.example.com.	1	IN	MX	10 subdomain.example.com.
    example.com.	1	IN	MX	20 in2-smtp.messagingengine.com.
    example.com.	1	IN	MX	10 in1-smtp.messagingengine.com.

    ;; SRV Records
    _servce._tcp.subdomain.example.com.	1	IN	SRV	0 200 443 servce.example.com.
    _servce._tcp.example.com.	1	IN	SRV	0 100 443 servce.example.com.
    _ldap._tcp.subdomain.example.com.	1	IN	SRV	0 100 636 internal.servce.example.com.
    _mqtt._tcp.subdomain.example.com.	1	IN	SRV	0 100 1883 mqtt.subdomain.example.com.
    _ntp._tcp.subdomain.example.com.	1	IN	SRV	0 100 123 ntp.subdomain.example.com.
    _ntp._udp.subdomain.example.com.	1	IN	SRV	0 100 123 ntp.subdomain.example.com.
    _proxy._tcp.subdomain.example.com.	1	IN	SRV	0 100 3128 proxy.subdomain.example.com.

    ;; TXT Records
    _dmarc.subdomain.example.com.	1	IN	TXT	"v=DMARC1; p=quarantine; rua=mailto:dmarc@donkeymonitor.com; fo=1; pct=100"
    _dmarc.example.com.	1	IN	TXT	"v=DMARC1; p=none; rua=mailto:example+dmarc@not-example.net"
    subdomain.example.com.	1	IN	TXT	"v=spf1 include:spf.messagingengine.com mx a ip4:202.101.40.5/32 ip6:2400:efe0::ffff:91ff:fedf:a845/64 ~all"
    _kerberos.subdomain.example.com.	1	IN	TXT	"subdomain.example.com"
    _network.subdomain.example.com.	1	IN	TXT	"2001:cafe:beef::"
    example.com.	1	IN	TXT	"keybase-site-verification=asdflaksjhflksjhfsad"
    example.com.	1	IN	TXT	"google-site-verification=asdflkjhasflksjhflskadjhflakjsdhf"
    example.com.	1	IN	TXT	"v=spf1 include:spf.messagingengine.com mx a ip4:202.101.40.5/32 ip6:2400:efe0::f03c:91ff:fedf:a845/64 ~all"
    "#;
    let res = parse_file(example_file);

    dbg!(&res);

    res.expect("Failed to parse file successfully");
}

#[test]
fn test_simple_soa() {
    if flexi_logger::Logger::try_with_str("debug")
        .unwrap()
        .start()
        .is_err()
    {
        println!("well, no logging for you!");
    };

    let example_file = r#"example.com	IN	SOA	dns1.example.com.	hostmaster.example.com. (
    2001062501 ; serial
    21600      ; refresh after 6 hours
    3600       ; retry after 1 hour
    604800     ; expire after 1 week
    86400 )    ; minimum TTL of 1 day"#;
    let res = parse_file(example_file).expect("failed to parse file");
    assert!(res.soarecord.is_some());

    let example_file = r#"example.com	IN	SOA	dns1.example.com.	hostmaster.example.com. 2001062501 21600 3600 604800 86400 ; minimum TTL of 1 day"#;
    let res = parse_file(example_file).expect("failed to parse file");
    assert!(res.soarecord.is_some());
}

#[test]
fn test_busted_files() {
    if flexi_logger::Logger::try_with_str("debug")
        .unwrap()
        .start()
        .is_err()
    {
        println!("well, no logging for you!");
    };

    let example_file = r#"@ hello world;;   "#;
    assert!(parse_file(example_file).is_err());

    let example_file = r#") hello world;;   "#;
    assert!(parse_file(example_file).is_err());

    // this should trigger the "oh no too many fields in the SOA record" bit
    let example_file = r#"example.com 3600	IN	SOA	hera.ns.cloudflare.com. dns.cloudflare.com. ( ; comments are great
        2045224174
        10000
        2400 ; oh man I love comments
        604800
        hello world
        3600
     )  "#;
    let res = parse_file(example_file);
    dbg!(&res);
    if res.is_err() {
        panic!("should have parsed the weird SOA record");
    };
}
