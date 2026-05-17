use core::fmt::Debug;

use num_enum::TryFromPrimitive;

#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
pub enum Errno {
    OperationNotPermitted = -1,
    NotFound = -2,
    IoError = -5,
    NotExecutable = -8,
    BadFileDescriptor = -9,
    NoChild = -10,
    PermissionDenied = -13,
    InvalidArgument = -22,
    NotADirectory = -20,
    IsADirectory = -21,
    OutOfMemory = -12,
    BadAdress = -14,
    AlreadyExists = -17,
    ValueOverflow = -75,
    NoSuchProcess = -999,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotADirectory,
    IsADirectory,
    AlreadyExistsFile,
    AlreadyExistsDir,
    InvalidPath,
    NotFound,
    PermissionDenied,
}

#[derive(Debug)]
pub enum ElfLoadError {
    FileSizeLargerThanMemSize,
    AlignNotPowerTwo,
    SegmentFileRangeOutOfBounds,
    SegmentVirtOutOfBounds,
    VirtAddrOverflow,
    NoEntryInAnySegment,
    ParserError,
    PhdrError,
    PhdrNotMappedByLoad,
}

#[derive(Debug)]
pub enum MapElfError {
    Overflow,
    OverlappingPage { va: u64 },
    OutOfFrames,
    MapFailed,
    FlagUpdateFailed,
    CopyTooLarge,
}

#[derive(Debug)]
pub enum ProcessError {
    Fs(FsError),
    Elf(ElfLoadError),
    Map(MapElfError),
    InvalidImage,
    OutOfMemory,
}

impl From<FsError> for ProcessError {
    fn from(value: FsError) -> Self {
        Self::Fs(value)
    }
}

impl From<ElfLoadError> for ProcessError {
    fn from(value: ElfLoadError) -> Self {
        Self::Elf(value)
    }
}

impl From<MapElfError> for ProcessError {
    fn from(value: MapElfError) -> Self {
        Self::Map(value)
    }
}

impl From<ProcessError> for Errno {
    fn from(e: ProcessError) -> Self {
        match e {
            ProcessError::OutOfMemory => Errno::OutOfMemory,
            ProcessError::InvalidImage => Errno::NotExecutable,

            ProcessError::Fs(fs) => match fs {
                FsError::NotFound => Errno::NotFound,
                FsError::InvalidPath => Errno::InvalidArgument,
                FsError::NotADirectory => Errno::NotADirectory,
                FsError::IsADirectory => Errno::IsADirectory,
                FsError::AlreadyExistsFile | FsError::AlreadyExistsDir => Errno::AlreadyExists,
                FsError::PermissionDenied => Errno::PermissionDenied,
            },

            ProcessError::Elf(_elf) => Errno::NotExecutable,

            ProcessError::Map(map) => match map {
                MapElfError::OutOfFrames => Errno::OutOfMemory,

                MapElfError::Overflow
                | MapElfError::OverlappingPage { .. }
                | MapElfError::MapFailed
                | MapElfError::FlagUpdateFailed
                | MapElfError::CopyTooLarge => Errno::IoError,
            },
        }
    }
}

impl From<FsError> for Errno {
    fn from(e: FsError) -> Self {
        match e {
            FsError::NotFound => Errno::NotFound,
            FsError::InvalidPath => Errno::InvalidArgument,
            FsError::NotADirectory => Errno::NotADirectory,
            FsError::IsADirectory => Errno::IsADirectory,
            FsError::AlreadyExistsFile | FsError::AlreadyExistsDir => Errno::AlreadyExists,
            FsError::PermissionDenied => Errno::PermissionDenied,
        }
    }
}
