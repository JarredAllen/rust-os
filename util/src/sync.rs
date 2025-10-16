//! Concurrency-related primitives

pub mod atomic;

use core::ops::{Deref, DerefMut};

/// Assert that a type is [`Sync`].
#[derive(Debug, Default)]
pub struct AssertSync<T: ?Sized>(pub T);

// SAFETY: Type asserts `Sync`.
unsafe impl<T: ?Sized> Sync for AssertSync<T> {}

impl<T: ?Sized> Deref for AssertSync<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: ?Sized> DerefMut for AssertSync<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
