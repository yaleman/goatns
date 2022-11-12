use crate::enums::ContactDetails;

#[test]
fn test_contactdetails() {
    let good_mastodon = ContactDetails::try_from("Mastodon:yaleman@mastodon.social".to_string());
    println!("{good_mastodon:?}");
    assert!(good_mastodon.is_ok());
    let good_twitter = ContactDetails::try_from("Twitter:@yaleman43381258");
    assert!(good_twitter.is_ok());
    let good_email = ContactDetails::try_from("Email:billy@dotgoat.net");
    assert!(good_email.is_ok());

    assert!(ContactDetails::try_from("asdfasdf").is_err());
    assert!(ContactDetails::try_from("foo:asdfasdf").is_err());
    assert!(ContactDetails::try_from("foo:asdfasdfÚ:asdfasdfd").is_err());
}
