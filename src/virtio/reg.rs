#![allow(unused, reason = "some registers aren't used on 32-bit")]

use super::VirtioBlockRegister;

register!(
    Magic(u32, 0x000, R),
    Version(u32, 0x004, R),
    DeviceId(u32, 0x008, R),
    DeviceFeatures(DeviceFeatureFlags, 0x010, R),
    DeviceFeaturesSelect(DeviceFeatureFlags, 0x014, W),
    QueueSelect(u32, 0x030, W),
    QueueSize(u32, 0x038, W),
    QueuePfn(u32, 0x040, RW),
    QueueReady(u32, 0x044, RW),
    QueueNotify(u32, 0x050, W),
    DeviceStatus(DeviceStatusFlags, 0x070, RW),
    /* These aren't available for legacy devices
    QueueDescriptorLow(u32, 0x080, W),
    QueueDescriptorHigh(u32, 0x084, W),
    QueueAvailableLow(u32, 0x090, W),
    QueueAvailableHigh(u32, 0x094, W),
    QueueUsedLow(u32, 0x0A0, W),
    QueueUsedHigh(u32, 0x0A4, W),
    */
    Capacity(u64, 0x100, R),
);

bitset::bitset!(
    pub(super) DeviceStatusFlags(u32) {
        Acknowledge = 0,
        Driver = 1,
        DriverOk = 2,
        FeaturesOk = 3,
        DeviceNeedsReset = 6,
        Failed = 7,
    }
);

bitset::bitset!(
    pub(super) DeviceFeatureFlags(u32) {
        SizeMax = 1,
        SegmentsMax = 2,
        Geometry = 4,
        ReadOnly = 5,
        BlockSize = 6,
        Flush = 9,
        Topology = 10,
        ConfigWce = 11,
        Discard = 13,
        WriteZeros = 14,
    }
);

macro_rules! register {
    ($(
        $regname:ident($regty:ty, $regoffset:expr, $rw:ident),
    )*) => {$(
        #[derive(Debug)]
        pub(super) struct $regname;
        // SAFETY: Macro asserts that these are valid.
        unsafe impl VirtioBlockRegister for $regname {
            const OFFSET: usize = $regoffset;
            type RegTy = $regty;

            const READABLE: bool = readable!($rw);
            const WRITABLE: bool = writable!($rw);
        }
    )*};
}

macro_rules! readable {
    (R) => {
        true
    };
    (W) => {
        false
    };
    (RW) => {
        true
    };
}

macro_rules! writable {
    (R) => {
        false
    };
    (W) => {
        true
    };
    (RW) => {
        true
    };
}

use {readable, register, writable};
