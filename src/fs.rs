//! Filesystem abstraction.

use crate::reply::{
    Payload, //
    ReplyAttr,
    ReplyBmap,
    ReplyCreate,
    ReplyData,
    ReplyEmpty,
    ReplyEntry,
    ReplyLk,
    ReplyOpen,
    ReplyOpendir,
    ReplyReadlink,
    ReplyStatfs,
    ReplyWrite,
    ReplyXattr,
};
use futures_io::AsyncWrite;
use futures_util::io::AsyncWriteExt;
use polyfuse_sys::abi::{
    fuse_attr, //
    fuse_file_lock,
    fuse_in_header,
    fuse_kstatfs,
    fuse_out_header,
};
use smallvec::SmallVec;
use std::{convert::TryFrom, ffi::OsStr, fmt, io, io::IoSlice};

#[derive(Debug)]
#[repr(transparent)]
pub struct FileAttr(fuse_attr);

impl TryFrom<libc::stat> for FileAttr {
    type Error = <fuse_attr as TryFrom<libc::stat>>::Error;

    fn try_from(st: libc::stat) -> Result<Self, Self::Error> {
        fuse_attr::try_from(st).map(Self)
    }
}

impl FileAttr {
    pub(crate) fn into_inner(self) -> fuse_attr {
        self.0
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct FsStatistics(fuse_kstatfs);

impl FsStatistics {
    pub(crate) fn into_inner(self) -> fuse_kstatfs {
        self.0
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct FileLock(fuse_file_lock);

impl FileLock {
    pub(crate) fn new(attr: &fuse_file_lock) -> &Self {
        unsafe { &*(attr as *const fuse_file_lock as *const Self) }
    }

    pub(crate) fn into_inner(self) -> fuse_file_lock {
        self.0
    }
}

/// The filesystem running on the user space.
#[async_trait::async_trait(?Send)]
pub trait Filesystem<T> {
    /// Handle a FUSE request from the kernel and reply with its result.
    async fn call(&self, cx: &mut Context<'_>, op: Operation<'_, T>) -> io::Result<()>
    where
        T: 'async_trait, // https://github.com/dtolnay/async-trait/issues/8
    {
        drop(op);
        cx.reply_err(libc::ENOSYS).await
    }
}

/// Contextural information about an incoming request.
pub struct Context<'a> {
    header: &'a fuse_in_header,
    writer: &'a mut (dyn AsyncWrite + Unpin),
}

impl fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Context").finish()
    }
}

impl<'a> Context<'a> {
    pub(crate) fn new(
        header: &'a fuse_in_header,
        writer: &'a mut (impl AsyncWrite + Unpin),
    ) -> Self {
        Self { header, writer }
    }

    /// Return the user ID of the calling process.
    pub fn uid(&self) -> u32 {
        self.header.uid
    }

    /// Return the group ID of the calling process.
    pub fn gid(&self) -> u32 {
        self.header.gid
    }

    /// Return the process ID of the calling process.
    pub fn pid(&self) -> u32 {
        self.header.pid
    }

    /// Reply to the kernel with an error code.
    pub async fn reply_err(&mut self, error: i32) -> io::Result<()> {
        self.send_reply(error, &[]).await
    }

    /// Reply to the kernel with the specified data.
    pub(crate) async fn send_reply(&mut self, error: i32, data: &[&[u8]]) -> io::Result<()> {
        let data_len: usize = data.iter().map(|t| t.len()).sum();

        let out_header = fuse_out_header {
            unique: self.header.unique,
            error: -error,
            len: u32::try_from(std::mem::size_of::<fuse_out_header>() + data_len).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("the total length of data is too long: {}", e),
                )
            })?,
        };

        let vec: SmallVec<[_; 4]> = Some(IoSlice::new(out_header.as_bytes()))
            .into_iter()
            .chain(data.iter().map(|t| IoSlice::new(&*t)))
            .collect();

        (*self.writer).write_vectored(&*vec).await?;

        Ok(())
    }
}

/// The kind of FUSE requests received from the kernel.
#[derive(Debug)]
pub enum Operation<'a, T> {
    /// Look up a directory entry by name.
    Lookup {
        parent: u64,
        name: &'a OsStr,
        reply: ReplyEntry,
    },

    /// Forget about inodes removed from the kernel's internal caches.
    Forget {
        nlookups: &'a [(u64, u64)], //
    },

    /// Get file attributes.
    Getattr {
        ino: u64,
        fh: Option<u64>,
        reply: ReplyAttr,
    },

    /// Set file attributes.
    Setattr {
        ino: u64,
        fh: Option<u64>,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<(u64, u32, bool)>,
        mtime: Option<(u64, u32, bool)>,
        ctime: Option<(u64, u32)>,
        lock_owner: Option<u64>,
        reply: ReplyAttr,
    },

    /// Read a symbolic link.
    Readlink { ino: u64, reply: ReplyReadlink },

    /// Create a symbolic link
    Symlink {
        parent: u64,
        name: &'a OsStr,
        link: &'a OsStr,
        reply: ReplyEntry,
    },

    /// Create a file node.
    Mknod {
        parent: u64,
        name: &'a OsStr,
        mode: u32,
        rdev: u32,
        umask: Option<u32>,
        reply: ReplyEntry,
    },

    /// Create a directory.
    Mkdir {
        parent: u64,
        name: &'a OsStr,
        mode: u32,
        umask: Option<u32>,
        reply: ReplyEntry,
    },

    /// Remove a file.
    Unlink {
        parent: u64,
        name: &'a OsStr,
        reply: ReplyEmpty,
    },

    /// Remove a directory.
    Rmdir {
        parent: u64,
        name: &'a OsStr,
        reply: ReplyEmpty,
    },

    /// Rename a file.
    Rename {
        parent: u64,
        name: &'a OsStr,
        newparent: u64,
        newname: &'a OsStr,
        flags: u32,
        reply: ReplyEmpty,
    },

    /// Create a hard link.
    Link {
        ino: u64,
        newparent: u64,
        newname: &'a OsStr,
        reply: ReplyEntry,
    },

    /// Open a file.
    Open {
        ino: u64,
        flags: u32,
        reply: ReplyOpen,
    },

    /// Read data from an opened file.
    Read {
        ino: u64,
        fh: u64,
        offset: u64,
        flags: u32,
        lock_owner: Option<u64>,
        reply: ReplyData,
    },

    /// Write data to an opened file.
    Write {
        ino: u64,
        fh: u64,
        offset: u64,
        data: T,
        size: u32,
        flags: u32,
        lock_owner: Option<u64>,
        reply: ReplyWrite,
    },

    /// Release an opened file.
    Release {
        ino: u64,
        fh: u64,
        flags: u32,
        lock_owner: Option<u64>,
        flush: bool,
        flock_release: bool,
        reply: ReplyEmpty,
    },

    /// Get the filesystem statistics.
    Statfs { ino: u64, reply: ReplyStatfs },

    /// Synchronize the file contents of an opened file.
    ///
    /// When the parameter `datasync` is true, only the
    /// file contents should be flushed and the metadata
    /// does not have to be flushed.
    Fsync {
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: ReplyEmpty,
    },

    /// Set an extended attribute.
    Setxattr {
        ino: u64,
        name: &'a OsStr,
        value: &'a [u8],
        flags: u32,
        reply: ReplyEmpty,
    },

    /// Get an extended attribute.
    ///
    /// The operation should send the length of attribute's value
    /// with `reply.size(n)` when `size` is equal to zero.
    Getxattr {
        ino: u64,
        name: &'a OsStr,
        size: u32,
        reply: ReplyXattr,
    },

    /// List extended attribute names.
    ///
    /// The attribute names must be seperated by a null character
    /// (i.e. `b'\0'`).
    ///
    /// The operation should send the length of attribute names
    /// with `reply.size(n)` when `size` is equal to zero.
    Listxattr {
        ino: u64,
        size: u32,
        reply: ReplyXattr,
    },

    /// Remove an extended attribute.
    Removexattr {
        ino: u64,
        name: &'a OsStr,
        reply: ReplyEmpty,
    },

    /// Close a file descriptor.
    Flush {
        ino: u64,
        fh: u64,
        lock_owner: u64,
        reply: ReplyEmpty,
    },

    /// Open a directory.
    Opendir {
        ino: u64,
        flags: u32,
        reply: ReplyOpendir,
    },

    /// Read contents from an opened directory.
    Readdir {
        ino: u64,
        fh: u64,
        offset: u64,
        reply: ReplyData,
    },

    /// Release an opened directory.
    Releasedir {
        ino: u64,
        fh: u64,
        flags: u32,
        reply: ReplyEmpty,
    },

    /// Synchronize an opened directory contents.
    ///
    /// When the parameter `datasync` is true, only the
    /// directory contents should be flushed and the metadata
    /// does not have to be flushed.
    Fsyncdir {
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: ReplyEmpty,
    },

    /// Test for a POSIX file lock.
    Getlk {
        ino: u64,
        fh: u64,
        owner: u64,
        lk: &'a FileLock,
        reply: ReplyLk,
    },

    /// Acquire, modify or release a POSIX file lock.
    Setlk {
        ino: u64,
        fh: u64,
        owner: u64,
        lk: &'a FileLock,
        sleep: bool,
        reply: ReplyEmpty,
    },

    /// Acquire, modify or release a BSD file lock.
    Flock {
        ino: u64,
        fh: u64,
        owner: u64,
        op: u32,
        reply: ReplyEmpty,
    },

    /// Check file access permissions.
    Access {
        ino: u64,
        mask: u32,
        reply: ReplyEmpty,
    },

    /// Create and open a file.
    Create {
        parent: u64,
        name: &'a OsStr,
        mode: u32,
        umask: Option<u32>,
        open_flags: u32,
        reply: ReplyCreate,
    },

    /// Map block index within a file to block index within device.
    ///
    /// This operation makes sense only for filesystems that use
    /// block devices, and is called only when the mount options
    /// contains `blkdev`.
    Bmap {
        ino: u64,
        block: u64,
        blocksize: u32,
        reply: ReplyBmap,
    },
    // ioctl
    // poll
    // notify_reply
    // batch_forget
    // fallocate
    // readdirplus
    // rename2
    // lseek
    // copy_file_range
}
