//! A macro for making bit sets (see [`bitset!`]).

#![no_std]

/// A macro for making bitsets.
#[macro_export]
macro_rules! bitset {
    (
        $( #[$set_meta:meta] )*
        $pub:vis $name:ident($repr:ty) {
            $(
                $( #[$bit_meta:meta] )*
                $bit:ident $( = $disc:expr)? ),*
            $(,)?
        }
    ) => {$crate::__macro_export::paste! {
            $( #[$set_meta] )*
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            #[repr(transparent)]
            $pub struct $name($repr);
            const _: () = {
                use ::core::ops::BitOr;

                /// Constructors
                impl $name {
                    $(
                        $( #[$bit_meta] )*
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

                    /// Get whether we contain any bit set in `other`.
                    pub const fn contains_any(self, other: Self) -> bool {
                        (self.0 | other.0) != 0
                    }

                    $(
                        $( #[$bit_meta] )*
                        pub const fn [< $bit:snake:lower >](self) -> bool {
                            self.contains(Self::[< $bit:snake:upper >])
                        }
                    )*

                    /// Get whether this set is empty.
                    pub const fn is_empty(&self) -> bool {
                        self.0 == 0
                    }
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

                impl ::core::fmt::Display for $name {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        f.write_str(::core::concat!(::core::stringify!($name), " { "))?;
                        $(
                            if self.[< $bit:snake:lower >]() {
                                f.write_str(::core::concat!(::core::stringify!($bit), " "))?;
                            }
                        )*
                        if self.0 & !Self::MASK != 0 {
                            f.write_str("<unknown bits> ")?;
                        }
                        f.write_str("}")
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

                // A note about bytemuck impls:
                // Using `bytemuck` functions to set bits not defined may result in weird behavior,
                // but the behavior will always be sound.

                // SAFETY:
                // `#[repr(transparent)]` around plain old data is plain old data.
                unsafe impl $crate::__macro_export::Pod for $name where $repr: $crate::__macro_export::Pod {}
                // SAFETY: All zeros is the empty value.
                unsafe impl $crate::__macro_export::Zeroable for $name where $repr: $crate::__macro_export::Zeroable  {}
            };
        }};
}

/// A trait for types from [`bitset!`].
///
/// TODO All functionality should be duplicated between the trait (allowing for generic code) and
/// inherent methods (so you don't have to import the trait).
pub trait BitSet: From<Self::Repr> + Into<Self::Repr> {
    /// The underlying representation for this value.
    type Repr;

    /// Get a reference to the inner value.
    fn as_inner(&self) -> &Self::Repr;

    /// Get a mutable reference to the inner value.
    ///
    /// You may experience unexpected behavior if you set bits on the inner value which don't match
    /// bits in the bit set, but the behavior will still be sound.
    fn as_inner_mut(&mut self) -> &mut Self::Repr;
}

#[doc(hidden)]
pub mod __macro_export {
    pub use paste::paste;

    pub use bytemuck::{Pod, Zeroable};
}
