//! Native owner-only ACLs and write-through replacement. No shell or ACL tools.
use super::{Error, Result};
use std::{
    ffi::c_void,
    os::windows::{ffi::OsStrExt, fs::MetadataExt, io::AsRawHandle},
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree},
    Security::{self, Authorization::*, *},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, GetFileInformationByHandle,
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn wide(path: &Path) -> Result<Vec<u16>> {
    let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
    if value.contains(&0) {
        return Err(Error::UnsafePath);
    }
    value.push(0);
    Ok(value)
}
// Token storage must be suitably aligned for TOKEN_USER and its embedded SID.
fn with_user<T>(action: impl FnOnce(PSID) -> Result<T>) -> Result<T> {
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(Error::Permissions);
        }
        let mut length = 0;
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut length);
        if length == 0 {
            CloseHandle(token);
            return Err(Error::Permissions);
        }
        let mut storage = vec![0u64; (length as usize).div_ceil(8)];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            storage.as_mut_ptr().cast(),
            length,
            &mut length,
        );
        CloseHandle(token);
        if ok == 0 {
            return Err(Error::Permissions);
        }
        let user = &*storage.as_ptr().cast::<TOKEN_USER>();
        action(user.User.Sid)
    }
}

pub(super) fn secure_new(path: &Path) -> Result<()> {
    reject_reparse(path)?;
    let name = wide(path)?;
    with_user(|sid| unsafe {
        let mut sid_text = ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut sid_text) == 0 {
            return Err(Error::Permissions);
        }
        let _sid_allocation = Local(sid_text.cast());
        let mut length = 0;
        while *sid_text.add(length) != 0 {
            length += 1;
        }
        let sid_text = String::from_utf16(std::slice::from_raw_parts(sid_text, length))
            .map_err(|_| Error::Permissions)?;
        let inheritance = if path.is_dir() { "OICI" } else { "" };
        let sddl: Vec<u16> = format!("O:{sid_text}D:P(A;{inheritance};FA;;;{sid_text})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        ) == 0
        {
            return Err(Error::Permissions);
        }
        let _descriptor = Local(descriptor);
        if SetFileSecurityW(
            name.as_ptr(),
            OWNER_SECURITY_INFORMATION
                | DACL_SECURITY_INFORMATION
                | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        ) == 0
        {
            return Err(Error::Permissions);
        }
        Ok(())
    })?;
    check_private(path)
}

fn reject_reparse(path: &Path) -> Result<()> {
    // Check every ancestor: junctions and mount-point reparse points are not
    // necessarily reported as Rust symlinks.
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|_| Error::Io)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(Error::UnsafePath);
        }
    }
    Ok(())
}

pub(super) fn check_private(path: &Path) -> Result<()> {
    reject_reparse(path)?;
    if !path.is_dir() {
        let file = std::fs::File::open(path).map_err(|_| Error::Io)?;
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
            return Err(Error::Io);
        }
        if information.nNumberOfLinks != 1 {
            return Err(Error::Permissions);
        }
    }
    let name = wide(path)?;
    with_user(|sid| unsafe {
        let mut owner = ptr::null_mut();
        let mut acl = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        if GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        ) != 0
        {
            return Err(Error::Permissions);
        }
        let _descriptor = Local(descriptor);
        if owner.is_null() || EqualSid(owner, sid) == 0 || acl.is_null() || (*acl).AceCount == 0 {
            return Err(Error::Permissions);
        }
        for index in 0..(*acl).AceCount {
            let mut raw = ptr::null_mut();
            if GetAce(acl, index.into(), &mut raw) == 0 || raw.is_null() {
                return Err(Error::Permissions);
            }
            let header = &*raw.cast::<ACE_HEADER>();
            // ACCESS_ALLOWED_ACE_TYPE = 0. Reject object/callback/unknown ACEs
            // instead of incorrectly treating them as harmless grants.
            if header.AceType != 0
                || (header.AceSize as usize) < std::mem::size_of::<ACCESS_ALLOWED_ACE>()
            {
                return Err(Error::Permissions);
            }
            let ace = &*raw.cast::<ACCESS_ALLOWED_ACE>();
            let trustee = ptr::addr_of!(ace.SidStart).cast_mut().cast();
            if Security::IsValidSid(trustee) == 0 || EqualSid(trustee, sid) == 0 {
                return Err(Error::Permissions);
            }
        }
        Ok(())
    })
}

pub(super) fn atomic_replace(temp: &Path, destination: &Path) -> Result<()> {
    reject_reparse(temp)?;
    if destination.exists() {
        check_private(destination)?;
    }
    let source = wide(temp)?;
    let target = wide(destination)?;
    // Caller has flushed the file. Windows has no portable directory fsync;
    // request write-through completion of the same-volume rename instead.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(Error::OutcomeUnknown);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_only_acl_and_replace() {
        let directory = tempfile::tempdir().unwrap();
        secure_new(directory.path()).unwrap();
        let source = directory.path().join("source");
        let destination = directory.path().join("destination");
        std::fs::write(&source, b"new").unwrap();
        secure_new(&source).unwrap();
        std::fs::write(&destination, b"old").unwrap();
        secure_new(&destination).unwrap();
        atomic_replace(&source, &destination).unwrap();
        check_private(&destination).unwrap();
        assert_eq!(std::fs::read(destination).unwrap(), b"new");
    }
    #[test]
    fn rejects_null_dacl() {
        let directory = tempfile::tempdir().unwrap();
        secure_new(directory.path()).unwrap();
        let file = directory.path().join("public");
        std::fs::write(&file, b"").unwrap();
        let name = wide(&file).unwrap();
        assert_eq!(
            unsafe {
                SetNamedSecurityInfoW(
                    name.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null(),
                    ptr::null(),
                )
            },
            0
        );
        assert!(matches!(check_private(&file), Err(Error::Permissions)));
        secure_new(&file).unwrap();
    }
    #[test]
    fn rejects_hardlinked_file() {
        let directory = tempfile::tempdir().unwrap();
        secure_new(directory.path()).unwrap();
        let file = directory.path().join("original");
        std::fs::write(&file, b"").unwrap();
        secure_new(&file).unwrap();
        std::fs::hard_link(&file, directory.path().join("alias")).unwrap();
        assert!(matches!(check_private(&file), Err(Error::Permissions)));
    }
}
