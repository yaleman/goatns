use std::str::FromStr;

use crate::enums::ContactDetails;

#[test]
fn test_contactdetails() {
    let good_mastodon = ContactDetails::try_from("Mastodon:yaleman@mastodon.social".to_string());
    println!("{good_mastodon:?}");
    assert!(good_mastodon.is_ok());
    let expected_mastodon = ContactDetails::Mastodon {
        contact: "yaleman".to_string(),
        server: "mastodon.social".to_string(),
    };
    assert_eq!(good_mastodon.unwrap(), expected_mastodon);

    let good_mastodon = ContactDetails::try_from("Mastodon:@yaleman@mastodon.social".to_string());
    println!("{good_mastodon:?}");
    assert!(good_mastodon.is_ok());
    let expected_mastodon = ContactDetails::Mastodon {
        contact: "yaleman".to_string(),
        server: "mastodon.social".to_string(),
    };
    assert_eq!(good_mastodon.unwrap(), expected_mastodon);

    let good_twitter = ContactDetails::try_from("Twitter:@yaleman43381258".to_string());
    assert!(good_twitter.is_ok());
    let good_email = ContactDetails::try_from("Email:billy@dotgoat.net".to_string());
    assert!(good_email.is_ok());

    assert!(ContactDetails::try_from("asdfasdf".to_string()).is_err());
    assert!(ContactDetails::try_from("foo:asdfasdf".to_string()).is_err());
    assert!(ContactDetails::try_from("foo:asdfasdfÚ:asdfasdfd".to_string()).is_err());
}

#[test]
fn test_contactdetails_deser() {
    use crate::config::ConfigFile;

    let configfile = r#"{
	"admin_contact" : "asdfadsf"
}
"#;
    print!("Parsing config file...");
    let configfile = ConfigFile::from_str(configfile).unwrap();
    println!("OK");

    println!(
        "Checking that {:?} = {:?}",
        configfile.admin_contact,
        ContactDetails::None
    );
    assert_eq!(configfile.admin_contact, ContactDetails::None);

    let configfile = r#"{
	"admin_contact" : "Mastodon:yaleman@mastodon.social"
}
"#;
    print!("Parsing config file...");
    let configfile = ConfigFile::from_str(configfile).unwrap();
    println!("OK");

    let expected_result: ContactDetails = ContactDetails::Mastodon {
        contact: "yaleman".to_string(),
        server: "mastodon.social".to_string(),
    };

    println!(
        "Checking that {:?} = {expected_result:?}",
        configfile.admin_contact
    );
    assert_eq!(configfile.admin_contact, expected_result);
}
