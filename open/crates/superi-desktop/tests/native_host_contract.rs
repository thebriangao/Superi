use superi_desktop::{DesktopLaunchOptions, DesktopSession};

#[test]
fn bounded_smoke_mode_is_explicit_and_never_the_product_default() {
    let product = DesktopLaunchOptions::parse(Vec::<String>::new()).expect("product options");
    assert!(!product.smoke());
    assert_eq!(product.logical_size(), (1440, 900));

    let smoke = DesktopLaunchOptions::parse(["--smoke".to_owned()]).expect("bounded smoke options");
    assert!(smoke.smoke());
    assert_eq!(smoke.logical_size(), (1440, 900));
}

#[test]
fn native_host_initializes_the_portable_session_owner() {
    let root = tempfile::tempdir().expect("temporary session");
    let session_path = root.path().join("portable");
    let options = DesktopLaunchOptions::parse([
        "--session-root".to_owned(),
        session_path.display().to_string(),
    ])
    .expect("session options");

    let session = DesktopSession::initialize(&options).expect("portable session");
    assert_eq!(options.session_root(), Some(session_path.as_path()));
    assert_eq!(session.owners().root(), session_path);
}
