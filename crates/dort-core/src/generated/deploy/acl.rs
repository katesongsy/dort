use crate::{Error, Result, error::io_error};
use rustix::io::Errno;
use std::{
    ffi::{c_int, c_void},
    fs::File,
    io,
    os::fd::AsRawFd,
    path::Path,
    ptr::{NonNull, null_mut},
};

// Darwin sys/acl.h. Directory add/delete, mutation, ACL and ownership rights.
const WRITE_RIGHTS: u64 =
    (1 << 2) | (1 << 4) | (1 << 5) | (1 << 6) | (1 << 8) | (1 << 10) | (1 << 12) | (1 << 13);
const ACL_TYPE_EXTENDED: c_int = 0x100;
const ACL_EXTENDED_ALLOW: c_int = 1;
const ACL_EXTENDED_DENY: c_int = 2;
const ACL_FIRST_ENTRY: c_int = 0;
const ACL_NEXT_ENTRY: c_int = -1;

unsafe extern "C" {
    fn acl_get_fd_np(fd: c_int, kind: c_int) -> *mut c_void;
    fn acl_get_entry(acl: *mut c_void, id: c_int, entry: *mut *mut c_void) -> c_int;
    fn acl_get_tag_type(entry: *mut c_void, tag: *mut c_int) -> c_int;
    fn acl_get_permset_mask_np(entry: *mut c_void, mask: *mut u64) -> c_int;
    fn acl_free(acl: *mut c_void) -> c_int;
}

struct Acl(NonNull<c_void>);
impl Drop for Acl {
    fn drop(&mut self) {
        // SAFETY: acl_get_fd_np transferred this allocation to us.
        unsafe { acl_free(self.0.as_ptr()) };
    }
}

pub(super) fn require_no_write_grants(directory: &File, path: &Path) -> Result<()> {
    if grants_access(directory, WRITE_RIGHTS)
        .map_err(|error| io_error("check folder access rules", path, error))?
    {
        return Err(Error::Invalid(format!(
            "Folder {} has access rules that can allow writes. Choose a folder without extra write permissions.",
            path.display()
        )));
    }
    Ok(())
}

pub(super) fn require_private_stage(file: &File, path: &Path) -> Result<()> {
    if grants_access(file, u64::MAX)
        .map_err(|error| io_error("check staged file access rules", path, error))?
    {
        return Err(Error::Invalid(
            "The destination folder grants extra access to new files. Choose a folder without inherited access grants.".into(),
        ));
    }
    Ok(())
}

fn grants_access(file: &File, rights: u64) -> io::Result<bool> {
    // SAFETY: the borrowed FD stays open throughout this call.
    let Some(acl) =
        NonNull::new(unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) }).map(Acl)
    else {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(code)
                if [Errno::NOENT, Errno::NOTSUP, Errno::OPNOTSUPP]
                    .into_iter()
                    .any(|allowed| code == allowed.raw_os_error()) =>
            {
                Ok(false)
            }
            _ => Err(error),
        };
    };
    let mut id = ACL_FIRST_ENTRY;
    loop {
        let mut entry = null_mut();
        // SAFETY: `acl` owns the allocation. Each entry is borrowed from it until the next call.
        if unsafe { acl_get_entry(acl.0.as_ptr(), id, &mut entry) } != 0 {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(Errno::INVAL.raw_os_error()) {
                Ok(false)
            } else {
                Err(error)
            };
        }
        id = ACL_NEXT_ENTRY;
        let mut tag = 0;
        let mut mask = 0;
        // SAFETY: the entry came from this live ACL; the outputs are writable local values.
        if unsafe { acl_get_tag_type(entry, &mut tag) } != 0
            || unsafe { acl_get_permset_mask_np(entry, &mut mask) } != 0
        {
            return Err(io::Error::last_os_error());
        }
        match tag {
            ACL_EXTENDED_ALLOW if mask & rights != 0 => return Ok(true),
            ACL_EXTENDED_ALLOW | ACL_EXTENDED_DENY => (),
            _ => return Err(io::Error::other("unknown folder or file access rule")),
        }
    }
}
