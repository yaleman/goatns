use crate::db::ZoneOwnership;

#[test]
fn test_zoneownership_serde() {
    let test_str = r#"{"id":1,"userid":1,"zoneid":1}"#;

    let zo: ZoneOwnership = serde_json::from_str(test_str).unwrap();
    assert_eq!(zo.id, Some(1));

    let test_str = r#"{"userid":1,"zoneid":1}"#;
    let zo: ZoneOwnership = serde_json::from_str(test_str).unwrap();
    assert_eq!(zo.id, None);

    let res = serde_json::to_string(&zo).unwrap();

    assert_eq!(res, test_str);
}
