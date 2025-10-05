//! An implementation of ext2

use crate::{
    alloc::KByteBuf,
    error::{Error, ErrorKind, Result},
    virtio::VirtioBlock,
};

pub struct Ext2<'a> {
    fs: VirtioBlock<'a>,
    /// The contents of the superblock.
    ///
    /// We reference this memory often, so we keep it cached instead of requiring a new disk read
    /// each time we're interested in any of it.
    superblock: KByteBuf,
}
impl<'a> Ext2<'a> {
    pub fn new(fs: VirtioBlock<'a>) -> Result<Self> {
        let mut this = Self {
            fs,
            superblock: KByteBuf::new_zeroed(1024)?,
        };
        for (sector_in_block, buf) in this
            .superblock
            .as_chunks_mut::<512>()
            .0
            .iter_mut()
            .enumerate()
        {
            this.fs.read_sector(buf, sector_in_block as u64 + 2)?;
        }
        this.superblock()
            .check_validity()
            .expect("Superblock has invalid data");
        Ok(this)
    }

    fn superblock(&self) -> Superblock {
        let alloc = self.superblock.as_ref();
        let superblock = core::ptr::from_ref(alloc).cast::<Superblock>();
        unsafe { superblock.read() }
    }

    fn inode(&mut self, inode_num: u32) -> Inode {
        let superblock = self.superblock();
        let group_num = inode_num.saturating_sub(1) / superblock.inodes_per_group;
        let group = self.block_group_descriptor(group_num);

        // TODO Check that the inode is used.

        let inode_block = group.inode_table_addr
            + (inode_num.saturating_sub(1) % superblock.inodes_per_group)
                / superblock.inodes_per_block();

        let inodes_per_sector = 512 / superblock.inode_size;

        let inode_sector = inode_block as u64 * (2 << superblock.block_size_raw)
            + ((inode_num.saturating_sub(1) % superblock.inodes_per_block())
                / inodes_per_sector as u32) as u64;

        let mut buf = [0; 512];
        self.fs
            .read_sector(&mut buf, inode_sector)
            .expect("Failed to read inode");

        let inode_index_in_sector = (inode_num.saturating_sub(1) as usize
            % inodes_per_sector as usize)
            * superblock.inode_size as usize;

        let inode_ptr = core::ptr::from_ref(&buf)
            .cast::<Inode>()
            .wrapping_byte_add(inode_index_in_sector);

        unsafe { core::ptr::read_unaligned(inode_ptr) }
    }

    fn read_dir(&mut self, dir_inode_num: u32) -> DirectoryEntryIter {
        let inode = self.inode(dir_inode_num);
        // TODO Check that it is a directory
        if inode.size_lower != 1024 {
            todo!("Support big directories");
        }
        DirectoryEntryIter {
            buf: self.read_block(inode.direct_block_pointers[0]),
            idx: 0,
        }
    }

    /// Get the inode number for a specific path, if present.
    pub fn lookup_path<'path>(
        &mut self,
        path_parts: impl IntoIterator<Item = &'path str>,
    ) -> Option<u32> {
        let mut inode_num = 2;
        for part in path_parts {
            inode_num = self.read_dir(inode_num).find_for_name(part)?.inode_num;
        }
        Some(inode_num)
    }

    pub fn read_file_from_offset(
        &mut self,
        inode_num: u32,
        mut offset: u64,
        mut buf: &mut [u8],
    ) -> Result<usize> {
        let inode = self.inode(inode_num);
        if buf.len() as u64 > inode.file_size() - offset {
            buf = &mut buf[..(inode.file_size() - offset) as usize];
        }
        let sector_buf = &mut [0; 512];
        let mut sector_num = (offset / 512) as u32;
        let mut write_len = 0;
        loop {
            if offset >= inode.file_size() {
                return Ok(write_len);
            }
            self.read_inode_sector(inode_num, sector_num, sector_buf)?;
            let this_write_len = buf.len().min(512);
            buf[..this_write_len].copy_from_slice(&sector_buf[..this_write_len]);
            buf = &mut buf[this_write_len..];
            write_len += this_write_len;
            offset += this_write_len as u64;
            sector_num += 1;
        }
    }

    fn read_inode_sector(
        &mut self,
        inode_num: u32,
        sector_num: u32,
        buf: &mut [u8; 512],
    ) -> Result<()> {
        let superblock = self.superblock();
        let inode = self.inode(inode_num);
        assert_eq!(inode.inode_type(), InodeType::RegularFile);
        let block_idx = sector_num / superblock.sectors_per_block();
        let block_num = *inode
            .direct_block_pointers
            .get(block_idx as usize)
            .ok_or_else(|| {
                log::error!("TODO Support indirect block pointers");
                Error::from(ErrorKind::Unsupported)
            })?;
        self.fs.read_sector(
            buf,
            block_num as u64 * superblock.sectors_per_block() as u64
                + sector_num as u64 % superblock.sectors_per_block() as u64,
        )?;
        Ok(())
    }

    fn block_group_descriptor(&mut self, group_num: u32) -> BlockGroupDescriptor {
        const DESCS_PER_SECTOR: usize = 512 / core::mem::size_of::<BlockGroupDescriptor>();
        let superblock = self.superblock();
        assert!(group_num < superblock.num_block_groups());
        let table_start_sector = 2 + superblock.block_size() / 512;
        let mut buf = [0; 512];
        self.fs
            .read_sector(
                &mut buf,
                table_start_sector + (group_num as u64 * DESCS_PER_SECTOR as u64) / 512,
            )
            .expect("Failed to read block descriptor table");
        let desc_ptr = core::ptr::from_ref(&buf)
            .cast::<BlockGroupDescriptor>()
            .wrapping_add(group_num as usize % DESCS_PER_SECTOR);
        unsafe { desc_ptr.read_unaligned() }
    }

    /// Read the given block number.
    ///
    /// This takes extra time to read the whole block, so only use this method if you actually need
    /// to get the whole block.
    fn read_block(&mut self, block_num: u32) -> KByteBuf {
        let mut buf =
            KByteBuf::new_zeroed(self.superblock().block_size() as usize).expect("Out of memory");
        let start_sector = block_num as u64 * self.superblock().sectors_per_block() as u64;
        for (sector_in_block, buf) in buf.as_chunks_mut().0.iter_mut().enumerate() {
            self.fs
                .read_sector(buf, start_sector + sector_in_block as u64)
                .expect("Failed to read sector of block");
        }
        buf
    }
}

struct DirectoryEntryIter {
    buf: KByteBuf,
    idx: usize,
}
impl DirectoryEntryIter {
    fn next(&mut self) -> Option<&DirectoryEntry> {
        if self.idx >= self.buf.len() {
            return None;
        }
        let entry_ptr = self
            .buf
            .as_ptr()
            .wrapping_byte_add(self.idx)
            .cast::<DirectoryEntryHeader>();
        self.idx += unsafe { &*entry_ptr }.entry_size as usize;
        // SAFETY:
        // If the filesystem is valid, then the memory is correct for this. And the return lifetime is tied to `self`, so it is valid for that long.
        Some(unsafe { DirectoryEntry::for_header(entry_ptr) })
    }

    /// Find the entry with this name, if one exists.
    ///
    /// After completion, this iterator is immediately after the found entry, or at the end if it
    /// couldn't be found.
    fn find_for_name(&mut self, name: &str) -> Option<DirectoryEntryHeader> {
        loop {
            let next_entry = self.next()?;
            if &next_entry.name == name {
                return Some(next_entry.header);
            }
        }
    }
}

#[repr(C)]
#[derive(Debug)]
struct Superblock {
    inode_count: u32,
    block_count: u32,
    super_user_blocks: u32,
    free_blocks: u32,
    free_inodes: u32,
    superblock_block_number: u32,
    block_size_raw: u32,
    fragment_size_raw: u32,
    blocks_per_group: u32,
    fragments_per_group: u32,
    inodes_per_group: u32,
    last_mount_time: u32,
    last_written_time: u32,
    mounts_since_consistency_check: u16,
    ext2_signature: u16,
    file_system_state: u16,
    error_handling_behavior: u16,
    minor_version: u16,
    last_consistency_check_time: u32,
    consistency_check_interval: u32,
    operating_system_creator_id: u32,
    major_version: u32,
    user_id_reserved_blocks: u16,
    group_id_reserved_blocks: u16,
    // Extended fields, only present if major version >= 1
    first_non_reserved_inode: u32,
    inode_size: u16,
    superblock_block_group_number: u16,
    /// Optional features for performance/reliability gains.
    optional_features: OptionalFeatures,
    /// Required features for reading or writing
    required_features: RequiredFeatures,
    /// Required features for writing (but reading is okay without these)
    read_only_features: ReadOnlyFeatures,
}
impl Superblock {
    /// Check that this superblock is consistent with what we can do.
    fn check_validity(&self) -> Result<()> {
        if self.inodes_per_group == 0 || self.blocks_per_group == 0 {
            return Err(ErrorKind::Io.into());
        }
        if self.inode_count.div_ceil(self.inodes_per_group)
            != self.block_count.div_ceil(self.blocks_per_group)
        {
            return Err(ErrorKind::Io.into());
        }
        if self.major_version != 1 {
            log::error!("Unsupported major version {}", self.major_version);
            return Err(ErrorKind::Unsupported.into());
        }
        if self.inode_size < 128 {
            log::error!("Unsupported short inodes");
            return Err(ErrorKind::Unsupported.into());
        }
        if !RequiredFeatures::SUPPORTED.contains(self.required_features) {
            log::error!("Unsupported required features {}", self.required_features);
            return Err(ErrorKind::Unsupported.into());
        }
        if !ReadOnlyFeatures::SUPPORTED.contains(self.read_only_features) {
            log::error!("Unsupported read_only features {}", self.read_only_features);
            return Err(ErrorKind::Unsupported.into());
        }
        Ok(())
    }

    /// Get the number of block groups.
    fn num_block_groups(&self) -> u32 {
        self.block_count.div_ceil(self.blocks_per_group)
    }

    fn block_size(&self) -> u64 {
        1024 << self.block_size_raw
    }

    fn sectors_per_block(&self) -> u32 {
        (self.block_size() / 512) as u32
    }

    fn inodes_per_block(&self) -> u32 {
        (self.block_size() / self.inode_size as u64) as u32
    }
}

#[repr(C)]
struct BlockGroupDescriptor {
    block_usage_bitmap_addr: u32,
    inode_usage_bitmap_addr: u32,
    inode_table_addr: u32,
    free_blocks: u16,
    free_inodes: u16,
    num_directories: u16,
    _unused: u16,
}

#[repr(C)]
#[derive(Debug)]
struct Inode {
    /// The file type and the permissions.
    ///
    /// The upper 4 bits are [`InodeType`] and the rest are [`Permissions`].
    type_and_permissions: u16,
    user_id: u16,
    size_lower: u32,
    last_access_time: u32,
    creation_time: u32,
    modification_time: u32,
    deletion_time: u32,
    group_id: u16,
    hard_link_count: u16,
    disk_sectors_used: u32,
    flags: InodeFlags,
    operating_system_specific_1: [u8; 4],
    direct_block_pointers: [u32; 12],
    singly_indirect_block_pointer: u32,
    doubly_indirect_block_pointer: u32,
    triply_indirect_block_pointer: u32,
    generation_number: u32,
    extended_attributes: u32,
    size_upper_or_directory_acl: u32,
    fragment_block_address: u32,
    operating_system_specific_2: [u8; 12],
}
impl Inode {
    fn file_size(&self) -> u64 {
        self.size_lower as u64 | ((self.size_upper_or_directory_acl as u64) << 32)
    }

    fn inode_type(&self) -> InodeType {
        match (self.type_and_permissions >> 12) & 0xF {
            1 => InodeType::Fifo,
            2 => InodeType::CharacterDevice,
            4 => InodeType::Directory,
            6 => InodeType::BlockDevice,
            8 => InodeType::RegularFile,
            10 => InodeType::SymbolicLink,
            12 => InodeType::UnixSocket,
            ty => unreachable!("Invalid inode type {ty}"),
        }
    }
}

bitset::bitset!(
    InodeFlags(u32) {
        SynchronousUpdates = 3,
        ImmutableFile = 4,
        AppendOnly = 5,
        NotInDump = 6,
        LastAccessTimeNotUpdated = 7,
        // Gap for reserved values
        HashIndexedDirectory = 16,
        AfsDirectory = 17,
        JournalFileData = 18,
    }
);

bitset::bitset!(
    Permissions(u16) {
        SetUserId = 11,
        SetGroupId = 10,
        Sticky = 9,
        UserRead = 8,
        UserWrite = 7,
        UserExecute = 6,
        GroupRead = 5,
        GroupWrite = 4,
        GroupExecute = 3,
        OtherRead = 2,
        OtherWrite = 1,
        OtherExecute = 0,
    }
);

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InodeType {
    Fifo = 1,
    CharacterDevice = 2,
    Directory = 4,
    BlockDevice = 6,
    RegularFile = 8,
    SymbolicLink = 10,
    UnixSocket = 12,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct DirectoryEntryHeader {
    inode_num: u32,
    entry_size: u16,
    name_len: u8,
    entry_type: u8,
}

#[repr(C)]
#[derive(Debug)]
struct DirectoryEntry {
    header: DirectoryEntryHeader,
    // TODO I don't think names have to be utf-8
    name: str,
}
impl DirectoryEntry {
    /// Create a [`DirectoryEntry`] for the given header.
    ///
    /// # Safety
    /// `header_ptr` must be valid for reading for `'a`, and also have provenance for the name
    /// section that follows.
    unsafe fn for_header<'a>(header_ptr: *const DirectoryEntryHeader) -> &'a Self {
        let len = unsafe { &*header_ptr }.name_len as usize;
        // We make a pointer to the value by first artificially constructing a pointer to a slice
        // with the right length. The slice pointer has the same format, so we can transmute.
        let similar_ptr = core::ptr::slice_from_raw_parts(header_ptr, len);
        let entry_ptr: *const Self = unsafe { core::mem::transmute(similar_ptr) };
        unsafe { &*entry_ptr }
    }
}

bitset::bitset!(
    OptionalFeatures(u32) {
        PreallocateToDirectory = 0,
        AfsServerInodes = 1,
        Journal = 2,
        InodeExtendedAttributes = 3,
        ResizeFilesystem = 4,
        HashIndex = 5,
    }
);
impl OptionalFeatures {
    const SUPPORTED: Self = Self::empty();
}

bitset::bitset!(
    RequiredFeatures(u32) {
        Compression = 0,
        DirectoryEntryType = 1,
        JournalReplay = 2,
        JournalDevice = 3,
    }
);

impl RequiredFeatures {
    const SUPPORTED: Self = Self::DIRECTORY_ENTRY_TYPE;
}

bitset::bitset!(
    ReadOnlyFeatures(u32) {
        SparseGroupDescriptors = 0,
        FileSize64Bit = 1,
        BinaryTreeDirectories = 2,
    }
);
impl ReadOnlyFeatures {
    const SUPPORTED: Self = Self::SPARSE_GROUP_DESCRIPTORS.bit_or(Self::FILE_SIZE64_BIT);
}
