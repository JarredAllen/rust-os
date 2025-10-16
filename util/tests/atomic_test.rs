//! Test coverage of the atomic type.

use bytemuck::NoUninit;
use util::sync::atomic::{Atomic, Ordering};

#[repr(u8)]
#[derive(NoUninit, Clone, Copy)]
enum Values {
    A,
    B,
    C,
}

#[derive(NoUninit, Clone, Copy)]
// Wider alignment needed to match `u32`
#[repr(C, align(4))]
struct TestDatum {
    a: u16,
    b: u8,
    c: Values,
}

#[test]
fn test_basic_atomic_ops() {
    let mut atomic = Atomic::new(TestDatum {
        a: 0,
        b: 0,
        c: Values::A,
    });
    assert!(
        matches!(
            atomic.load(Ordering::Relaxed),
            TestDatum {
                a: 0,
                b: 0,
                c: Values::A,
            }
        ),
        "`load` didn't match expected value",
    );
    atomic.store(
        TestDatum {
            a: 1,
            b: 2,
            c: Values::C,
        },
        Ordering::Relaxed,
    );
    assert!(
        matches!(
            atomic.load(Ordering::Relaxed),
            TestDatum {
                a: 1,
                b: 2,
                c: Values::C,
            }
        ),
        "`store` didn't change value",
    );
    assert!(
        atomic
            .update_weak(Ordering::Relaxed, Ordering::Relaxed, |mut old_value| {
                old_value.c = Values::B;
                old_value
            })
            .is_ok(),
        "Update failed without concurrent access"
    );
    assert!(
        matches!(
            atomic.update(Ordering::Relaxed, Ordering::Relaxed, |mut old_value| {
                old_value.c = Values::C;
                old_value
            }),
            TestDatum {
                a: 1,
                b: 2,
                c: Values::B,
            }
        ),
        "`update_weak` didn't change value to expected",
    );
    assert!(
        matches!(
            atomic.load(Ordering::Relaxed),
            TestDatum {
                a: 1,
                b: 2,
                c: Values::C,
            }
        ),
        "`update` didn't change value to expected",
    );
    assert!(
        matches!(
            atomic.get_mut(),
            &mut TestDatum {
                a: 1,
                b: 2,
                c: Values::C,
            }
        ),
        "`get_mut` didn't read value as expected",
    );
    assert!(
        matches!(
            atomic.into_inner(),
            TestDatum {
                a: 1,
                b: 2,
                c: Values::C,
            }
        ),
        "`into_inner` didn't read value as expected",
    );
}

#[test]
fn test_bitwise_ops() {
    let atomic = Atomic::new(0_u8);
    assert_eq!(atomic.fetch_or(0x13, Ordering::Relaxed), 0);
    assert_eq!(atomic.fetch_xor(0x22, Ordering::Relaxed), 0x13);
    assert_eq!(atomic.fetch_and(0x11, Ordering::Relaxed), 0x31);
    assert_eq!(atomic.fetch_nand(0x27, Ordering::Relaxed), 0x11);
    assert_eq!(atomic.load(Ordering::Relaxed), 0xfe);
}
