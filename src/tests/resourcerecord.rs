use crate::resourcerecord::check_long_labels;

#[test]
fn test_check_long_labels() {
    assert!(!check_long_labels("hello"));
    assert!(!check_long_labels("hello.world"));
    assert!(check_long_labels(
        "foo.12345678901234567890123456789012345678901234567890123456789012345678901234567890"
    ));
}
