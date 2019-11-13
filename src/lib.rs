//! FUSE (Filesystem in userspace) framework for Rust.

#![warn(clippy::checked_conversions)]
#![deny(
    missing_debug_implementations,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::invalid_upcast_comparisons,
    clippy::unimplemented
)]

pub mod reply;
pub mod request;

mod common;
mod dirent;
mod fs;
mod init;
mod session;

#[doc(inline)]
pub use crate::{
    common::{FileAttr, FileLock, Forget, FsStatistics},
    dirent::{DirEntry, DirEntryType},
    fs::{Context, Filesystem, Operation},
    init::{CapabilityFlags, ConnectionInfo, SessionInitializer},
    request::Buffer,
    session::{Interrupt, NotifyRetrieve, Session},
};
