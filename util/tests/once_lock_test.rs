//! Testing of [`OnceLock`].

use util::cell::OnceLock;

#[test]
fn test_once_lock() {
    let lock = OnceLock::<u32>::default();
    assert!(lock.get().is_none());
    assert!(lock.set(5).is_ok());
    assert_eq!(*lock.get().expect("Should now have a value"), 5);
    assert!(lock.set(6).is_err(), "Should no longer allow setting");

    let lock = OnceLock::from(7_u32);
    assert_eq!(*lock.get().expect("Should now have a value"), 7);
    assert!(lock.set(8).is_err(), "Should no longer allow setting");
}
