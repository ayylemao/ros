pub const PROT_READ: i32 = 0x1;
pub const PROT_WRITE: i32 = 0x2;
pub const PROT_EXEC: i32 = 0x4;

pub const MAP_PRIVATE: i32 = 0x02;
pub const MAP_FIXED: i32 = 0x10;
pub const MAP_ANONYMOUS: i32 = 0x20;

#[derive(Clone, Debug)]
pub struct MmapRegion {
    pub start: u64,
    pub len: u64,
    pub prot: i32,
    pub flags: i32,
}
