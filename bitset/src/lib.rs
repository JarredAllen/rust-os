#![no_std]

#[macro_export]
macro_rules! bitset {
    (
        $pub:vis $name:ident($repr:ty) {
            $(
                // TODO Allow doc comments here
                $bit:ident $( = $disc:expr)? ),*
            $(,)?
        }
    ) => {$crate::__macro_export::paste! {
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            $pub struct $name($repr);
            const _: () = {
                use ::core::ops::BitOr;

                /// Constructors
                impl $name {
                    $(
                        pub const [< $bit:snake:upper >]: Self = Self(1 << (Offsets::$bit as usize));
                    )*

                    /// Make a value with no bits set.
                    pub const fn empty() -> Self { Self(0) }

                    /// Make a value with every bit set.
                    pub const fn all() -> Self { Self(const { $( Self::[< $bit:snake:upper >].0 |)* 0 }) }

                    /// The raw bits set in [`Self::all`].
                    const MASK: $repr = Self::all().0;
                }

                /// Functions for manipulating values.
                impl $name {
                    /// Get all bits set in either input.
                    pub const fn bit_or(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }

                    /// Get whether we contain every bit set in `other`.
                    pub const fn contains(self, other: Self) -> bool {
                        (self.0 & other.0) == other.0
                    }

                    $(
                        pub const fn [< $bit:snake:lower >](self) -> bool {
                            self.contains(Self::[< $bit:snake:upper >])
                        }
                    )*
                }
                /// Combine the bits from each.
                ///
                /// See [`Self::bit_or`] for a const-time implementation.
                impl BitOr for $name {
                    type Output = Self;
                    fn bitor(self, rhs: Self) -> Self::Output {
                        self.bit_or(rhs)
                    }
                }

                impl From<$repr> for $name {
                    fn from(repr: $repr) -> Self {
                        Self(repr & Self::MASK)
                    }
                }
                impl From<$name> for $repr {
                    fn from(bitset: $name) -> $repr {
                        bitset.0
                    }
                }

                /// Partial ordering by each bit, `a > b` implies every bit set in `b` is also set
                /// in `a`.
                ///
                /// See [`Self::contains`] for a const-time implementation.
                impl PartialOrd for $name {
                    fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
                        if self == rhs {
                            Some(core::cmp::Ordering::Equal)
                        } else
                        if self.contains(*rhs) {
                            Some(core::cmp::Ordering::Greater)
                        } else
                        if rhs.contains(*self) {
                            Some(core::cmp::Ordering::Less)
                        } else {
                            None
                        }
                    }
                }

                /// Default to an empty set of values.
                impl ::core::default::Default for $name {
                    fn default() -> Self {
                        Self::empty()
                    }
                }

                impl $crate::BitSet for $name {
                    type Repr = $repr;

                    fn as_inner(&self) -> &Self::Repr { &self.0 }
                    fn as_inner_mut(&mut self) -> &mut Self::Repr { &mut self.0 }
                }

                /// Use an enum to generate offsets if not provided.
                enum Offsets {
                    $( $bit $( = $disc )? ),*
                }
            };
        }};
}

/// A trait for types from [`bitset!`].
///
/// TODO All functionality should be duplicated between the trait (allowing for generic code) and
/// inherent methods (so you don't have to import the trait).
pub trait BitSet: From<Self::Repr> + Into<Self::Repr> {
    type Repr;

    fn as_inner(&self) -> &Self::Repr;
    fn as_inner_mut(&mut self) -> &mut Self::Repr;
}

#[doc(hidden)]
pub mod __macro_export {
    pub use paste::paste;
}
