// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Set of functions to manage file systems

// spell-checker:ignore DATETIME getmntinfo subsecond (fs) cifs smbfs

#[cfg(any(target_os = "linux", target_os = "android"))]
const LINUX_MTAB: &str = "/etc/mtab";
#[cfg(any(target_os = "linux", target_os = "android"))]
const LINUX_MOUNTINFO: &str = "/proc/self/mountinfo";
#[cfg(windows)]
const MAX_PATH: usize = 266;
#[cfg(windows)]
static EXIT_ERR: i32 = 1;

#[cfg(any(
  windows,
  target_os = "freebsd",
  target_vendor = "apple",
  target_os = "netbsd",
  target_os = "openbsd"
))]
#[cfg(windows)]
use crate::show_warning;

use anyhow::Result;
#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows_sys::Win32::{
  Foundation::{ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
  Storage::FileSystem::{
    FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceW, GetDriveTypeW,
    GetVolumeInformationW, GetVolumePathNamesForVolumeNameW, QueryDosDeviceW,
  },
  System::WindowsProgramming::DRIVE_REMOTE,
};

#[cfg(windows)]
#[allow(non_snake_case)]
fn LPWSTR2String(buf: &[u16]) -> String {
  let len = buf.iter().position(|&n| n == 0).unwrap();
  String::from_utf16(&buf[..len]).unwrap()
}

#[cfg(windows)]
fn to_nul_terminated_wide_string(s: impl AsRef<OsStr>) -> Vec<u16> {
  s.as_ref()
    .encode_wide()
    .chain(Some(0))
    .collect::<Vec<u16>>()
}

#[cfg(unix)]
use libc::strerror;
#[cfg(unix)]
use std::ffi::CStr;
#[cfg(unix)]
use std::ffi::CString;
use std::io::Error as IOError;
#[cfg(unix)]
use std::mem;
#[cfg(windows)]
use std::path::Path;

#[cfg(any(
  target_os = "linux",
  target_os = "android",
  target_vendor = "apple",
  target_os = "freebsd",
  target_os = "openbsd"
))]
pub use libc::statfs as StatFs;
#[cfg(any(
  target_os = "aix",
  target_os = "netbsd",
  target_os = "dragonfly",
  target_os = "illumos",
  target_os = "solaris",
  target_os = "redox"
))]
pub use libc::statvfs as StatFs;

#[cfg(any(
  target_os = "linux",
  target_os = "android",
  target_vendor = "apple",
  target_os = "freebsd",
  target_os = "openbsd",
))]
pub use libc::statfs as statfs_fn;
#[cfg(any(
  target_os = "aix",
  target_os = "netbsd",
  target_os = "illumos",
  target_os = "solaris",
  target_os = "dragonfly",
  target_os = "redox"
))]
pub use libc::statvfs as statfs_fn;

#[derive(Debug, Clone)]
pub struct MountInfo {
  /// Stores `volume_name` in windows platform and `dev_id` in unix platform
  pub dev_name: String,
  pub fs_type: String,
  pub mount_dir: String,
}

impl MountInfo {
  #[cfg(any(target_os = "linux", target_os = "android"))]
  fn new(file_name: &str, raw: &[&str]) -> Option<Self> {
    let dev_name;
    let fs_type;
    let mount_dir;

    match file_name {
      // spell-checker:ignore (word) noatime
      // Format: 36 35 98:0 /mnt1 /mnt2 rw,noatime master:1 - ext3 /dev/root rw,errors=continue
      // "man proc" for more details
      LINUX_MOUNTINFO => {
        const FIELDS_OFFSET: usize = 6;
        let after_fields =
          raw[FIELDS_OFFSET..].iter().position(|c| *c == "-").unwrap() + FIELDS_OFFSET + 1;
        dev_name = raw[after_fields + 1].to_string();
        fs_type = raw[after_fields].to_string();
        mount_dir = raw[4].to_string();
      }
      LINUX_MTAB => {
        dev_name = raw[0].to_string();
        fs_type = raw[2].to_string();
        mount_dir = raw[1].to_string();
      }
      _ => return None,
    };

    Some(Self {
      dev_name,
      fs_type,
      mount_dir,
    })
  }

  #[cfg(windows)]
  fn new(mut volume_name: String) -> Option<Self> {
    let mut dev_name_buf = [0u16; MAX_PATH];
    volume_name.pop();
    unsafe {
      QueryDosDeviceW(
        OsStr::new(&volume_name)
          .encode_wide()
          .chain(Some(0))
          .skip(4)
          .collect::<Vec<u16>>()
          .as_ptr(),
        dev_name_buf.as_mut_ptr(),
        dev_name_buf.len() as u32,
      )
    };
    volume_name.push('\\');
    let dev_name = LPWSTR2String(&dev_name_buf);

    let mut mount_root_buf = [0u16; MAX_PATH];
    let success = unsafe {
      let volume_name = to_nul_terminated_wide_string(&volume_name);
      GetVolumePathNamesForVolumeNameW(
        volume_name.as_ptr(),
        mount_root_buf.as_mut_ptr(),
        mount_root_buf.len() as u32,
        ptr::null_mut(),
      )
    };
    if 0 == success {
      // TODO: support the case when `GetLastError()` returns `ERROR_MORE_DATA`
      return None;
    }
    let mount_root = LPWSTR2String(&mount_root_buf);

    let mut fs_type_buf = [0u16; MAX_PATH];
    let success = unsafe {
      let mount_root = to_nul_terminated_wide_string(&mount_root);
      GetVolumeInformationW(
        mount_root.as_ptr(),
        ptr::null_mut(),
        0,
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
        fs_type_buf.as_mut_ptr(),
        fs_type_buf.len() as u32,
      )
    };
    let fs_type = if 0 == success {
      None
    } else {
      Some(LPWSTR2String(&fs_type_buf))
    };
    let remote = DRIVE_REMOTE
      == unsafe {
        let mount_root = to_nul_terminated_wide_string(&mount_root);
        GetDriveTypeW(mount_root.as_ptr())
      };
    Some(Self {
      dev_id: volume_name,
      dev_name,
      fs_type: fs_type.unwrap_or_default(),
      mount_root,
      mount_dir: String::new(),
      mount_option: String::new(),
      remote,
      dummy: false,
    })
  }
}

#[cfg(any(
  target_os = "freebsd",
  target_vendor = "apple",
  target_os = "netbsd",
  target_os = "openbsd",
))]
impl From<StatFs> for MountInfo {
  fn from(statfs: StatFs) -> Self {
    let dev_name = unsafe {
      // spell-checker:disable-next-line
      CStr::from_ptr(&statfs.f_mntfromname[0])
        .to_string_lossy()
        .into_owned()
    };
    let fs_type = unsafe {
      // spell-checker:disable-next-line
      CStr::from_ptr(&statfs.f_fstypename[0])
        .to_string_lossy()
        .into_owned()
    };
    let mount_dir = unsafe {
      // spell-checker:disable-next-line
      CStr::from_ptr(&statfs.f_mntonname[0])
        .to_string_lossy()
        .into_owned()
    };

    let dev_id = mount_dev_id(&mount_dir);
    let dummy = is_dummy_filesystem(&fs_type, "");
    let remote = is_remote_filesystem(&dev_name, &fs_type);

    Self {
      dev_id,
      dev_name,
      fs_type,
      mount_dir,
      mount_root: String::new(),
      mount_option: String::new(),
      remote,
      dummy,
    }
  }
}

#[cfg(any(
  target_os = "freebsd",
  target_vendor = "apple",
  target_os = "netbsd",
  target_os = "openbsd"
))]
use libc::c_int;
#[cfg(any(
  target_os = "freebsd",
  target_vendor = "apple",
  target_os = "netbsd",
  target_os = "openbsd"
))]
extern "C" {
  #[cfg(all(target_vendor = "apple", target_arch = "x86_64"))]
  #[link_name = "getmntinfo$INODE64"]
  fn get_mount_info(mount_buffer_p: *mut *mut StatFs, flags: c_int) -> c_int;

  #[cfg(any(
    target_os = "netbsd",
    target_os = "openbsd",
    all(target_vendor = "apple", target_arch = "aarch64")
  ))]
  #[link_name = "getmntinfo"]
  fn get_mount_info(mount_buffer_p: *mut *mut StatFs, flags: c_int) -> c_int;

  // Rust on FreeBSD uses 11.x ABI for filesystem metadata syscalls.
  // Call the right version of the symbol for getmntinfo() result to
  // match libc StatFS layout.
  #[cfg(target_os = "freebsd")]
  #[link_name = "getmntinfo@FBSD_1.0"]
  fn get_mount_info(mount_buffer_p: *mut *mut StatFs, flags: c_int) -> c_int;
}

//use crate::error::UResult;
// #[cfg(any(
//   target_os = "freebsd",
//   target_vendor = "apple",
//   target_os = "netbsd",
//   target_os = "openbsd",
//   target_os = "windows"
// ))]
// use crate::error::USimpleError;
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::fs::File;
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::io::{BufRead, BufReader};
#[cfg(any(
  target_vendor = "apple",
  target_os = "freebsd",
  target_os = "windows",
  target_os = "netbsd",
  target_os = "openbsd"
))]
use std::ptr;
#[cfg(any(
  target_vendor = "apple",
  target_os = "freebsd",
  target_os = "netbsd",
  target_os = "openbsd"
))]
use std::slice;

/// Read file system list.
pub fn read_fs_list() -> Result<Vec<MountInfo>> {
  #[cfg(any(target_os = "linux", target_os = "android"))]
  {
    let (file_name, f) = File::open(LINUX_MOUNTINFO)
      .map(|f| (LINUX_MOUNTINFO, f))
      .or_else(|_| File::open(LINUX_MTAB).map(|f| (LINUX_MTAB, f)))?;
    let reader = BufReader::new(f);
    Ok(
      reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| {
          let raw_data = line.split_whitespace().collect::<Vec<&str>>();
          MountInfo::new(file_name, &raw_data)
        })
        .collect::<Vec<_>>(),
    )
  }
  #[cfg(any(
    target_os = "freebsd",
    target_vendor = "apple",
    target_os = "netbsd",
    target_os = "openbsd"
  ))]
  {
    let mut mount_buffer_ptr: *mut StatFs = ptr::null_mut();
    let len = unsafe { get_mount_info(&mut mount_buffer_ptr, 1_i32) };
    if len < 0 {
      return Err(USimpleError::new(1, "get_mount_info() failed"));
    }
    let mounts = unsafe { slice::from_raw_parts(mount_buffer_ptr, len as usize) };
    Ok(
      mounts
        .iter()
        .map(|m| MountInfo::from(*m))
        .collect::<Vec<_>>(),
    )
  }
  #[cfg(windows)]
  {
    let mut volume_name_buf = [0u16; MAX_PATH];
    // As recommended in the MS documentation, retrieve the first volume before the others
    let find_handle =
      unsafe { FindFirstVolumeW(volume_name_buf.as_mut_ptr(), volume_name_buf.len() as u32) };
    if INVALID_HANDLE_VALUE == find_handle {
      let os_err = IOError::last_os_error();
      let msg = format!("FindFirstVolumeW failed: {}", os_err);
      return Err(USimpleError::new(EXIT_ERR, msg));
    }
    let mut mounts = Vec::<MountInfo>::new();
    loop {
      let volume_name = LPWSTR2String(&volume_name_buf);
      if !volume_name.starts_with("\\\\?\\") || !volume_name.ends_with('\\') {
        show_warning!("A bad path was skipped: {}", volume_name);
        continue;
      }
      if let Some(m) = MountInfo::new(volume_name) {
        mounts.push(m);
      }
      if 0
        == unsafe {
          FindNextVolumeW(
            find_handle,
            volume_name_buf.as_mut_ptr(),
            volume_name_buf.len() as u32,
          )
        }
      {
        let err = IOError::last_os_error();
        if err.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
          let msg = format!("FindNextVolumeW failed: {err}");
          return Err(USimpleError::new(EXIT_ERR, msg));
        }
        break;
      }
    }
    unsafe {
      FindVolumeClose(find_handle);
    }
    Ok(mounts)
  }
  #[cfg(any(
    target_os = "aix",
    target_os = "redox",
    target_os = "illumos",
    target_os = "solaris"
  ))]
  {
    // No method to read mounts, yet
    Ok(Vec::new())
  }
}

#[derive(Debug, Clone)]
pub struct FsUsage {
  pub blocksize: u64,
  pub blocks: u64,
  pub bfree: u64,
  pub bavail: u64,
}

impl FsUsage {
  #[cfg(unix)]
  pub fn new(statvfs: StatFs) -> Self {
    {
      #[cfg(all(
        not(any(target_os = "freebsd", target_os = "openbsd")),
        target_pointer_width = "64"
      ))]
      return Self {
        blocksize: statvfs.f_bsize as u64, // or `statvfs.f_frsize` ?
        blocks: statvfs.f_blocks,
        bfree: statvfs.f_bfree,
        bavail: statvfs.f_bavail,
      };
      #[cfg(all(
        not(any(target_os = "freebsd", target_os = "openbsd")),
        not(target_pointer_width = "64")
      ))]
      return Self {
        blocksize: statvfs.f_bsize as u64, // or `statvfs.f_frsize` ?
        blocks: statvfs.f_blocks.into(),
        bfree: statvfs.f_bfree.into(),
        bavail: statvfs.f_bavail.into(),
        bavail_top_bit_set: ((statvfs.f_bavail as u64) & (1u64.rotate_right(1))) != 0,
        files: statvfs.f_files.into(),
        ffree: statvfs.f_ffree.into(),
      };
      #[cfg(target_os = "freebsd")]
      return Self {
        blocksize: statvfs.f_bsize, // or `statvfs.f_frsize` ?
        blocks: statvfs.f_blocks,
        bfree: statvfs.f_bfree,
        bavail: statvfs.f_bavail.try_into().unwrap(),
        bavail_top_bit_set: ((std::convert::TryInto::<u64>::try_into(statvfs.f_bavail).unwrap())
          & (1u64.rotate_right(1)))
          != 0,
        files: statvfs.f_files,
        ffree: statvfs.f_ffree.try_into().unwrap(),
      };
      #[cfg(target_os = "openbsd")]
      return Self {
        blocksize: statvfs.f_bsize.into(),
        blocks: statvfs.f_blocks,
        bfree: statvfs.f_bfree,
        bavail: statvfs.f_bavail.try_into().unwrap(),
        bavail_top_bit_set: ((std::convert::TryInto::<u64>::try_into(statvfs.f_bavail).unwrap())
          & (1u64.rotate_right(1)))
          != 0,
        files: statvfs.f_files,
        ffree: statvfs.f_ffree,
      };
    }
  }
  #[cfg(windows)]
  pub fn new(path: &Path) -> UResult<Self> {
    let mut root_path = [0u16; MAX_PATH];
    let success = unsafe {
      let path = to_nul_terminated_wide_string(path);
      GetVolumePathNamesForVolumeNameW(
        //path_utf8.as_ptr(),
        path.as_ptr(),
        root_path.as_mut_ptr(),
        root_path.len() as u32,
        ptr::null_mut(),
      )
    };
    if 0 == success {
      let msg = format!(
        "GetVolumePathNamesForVolumeNameW failed: {}",
        IOError::last_os_error()
      );
      return Err(USimpleError::new(EXIT_ERR, msg));
    }

    let mut sectors_per_cluster = 0;
    let mut bytes_per_sector = 0;
    let mut number_of_free_clusters = 0;
    let mut total_number_of_clusters = 0;

    let success = unsafe {
      let path = to_nul_terminated_wide_string(path);
      GetDiskFreeSpaceW(
        path.as_ptr(),
        &mut sectors_per_cluster,
        &mut bytes_per_sector,
        &mut number_of_free_clusters,
        &mut total_number_of_clusters,
      )
    };
    if 0 == success {
      // Fails in case of CD for example
      // crash!(
      //     EXIT_ERR,
      //     "GetDiskFreeSpaceW failed: {}",
      //     IOError::last_os_error()
      // );
    }

    let bytes_per_cluster = sectors_per_cluster as u64 * bytes_per_sector as u64;
    Ok(Self {
      // f_bsize      File system block size.
      blocksize: bytes_per_cluster,
      // f_blocks - Total number of blocks on the file system, in units of f_frsize.
      // frsize =     Fundamental file system block size (fragment size).
      blocks: total_number_of_clusters as u64,
      //  Total number of free blocks.
      bfree: number_of_free_clusters as u64,
      //  Total number of free blocks available to non-privileged processes.
      bavail: 0,
      bavail_top_bit_set: ((bytes_per_sector as u64) & (1u64.rotate_right(1))) != 0,
      // Total number of file nodes (inodes) on the file system.
      files: 0, // Not available on windows
      // Total number of free file nodes (inodes).
      ffree: 0, // Meaningless on Windows
    })
  }
}

#[cfg(unix)]
pub fn statfs<P>(path: P) -> Result<StatFs, String>
where
  P: Into<Vec<u8>>,
{
  match CString::new(path) {
    Ok(p) => {
      let mut buffer: StatFs = unsafe { mem::zeroed() };
      unsafe {
        match statfs_fn(p.as_ptr(), &mut buffer) {
          0 => Ok(buffer),
          _ => {
            let errno = IOError::last_os_error().raw_os_error().unwrap_or(0);
            Err(
              CStr::from_ptr(strerror(errno))
                .to_str()
                .map_err(|_| "Error message contains invalid UTF-8".to_owned())?
                .to_owned(),
            )
          }
        }
      }
    }
    Err(e) => Err(e.to_string()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[cfg(any(target_os = "linux", target_os = "android"))]
  fn test_mountinfo() {
    // spell-checker:ignore (word) relatime
    let info = MountInfo::new(
      LINUX_MOUNTINFO,
      &"106 109 253:6 / /mnt rw,relatime - xfs /dev/fs0 rw"
        .split_ascii_whitespace()
        .collect::<Vec<_>>(),
    )
    .unwrap();

    assert_eq!(info.mount_dir, "/mnt");
    assert_eq!(info.fs_type, "xfs");
    assert_eq!(info.dev_name, "/dev/fs0");

    // Test parsing with different amounts of optional fields.
    let info = MountInfo::new(
      LINUX_MOUNTINFO,
      &"106 109 253:6 / /mnt rw,relatime master:1 - xfs /dev/fs0 rw"
        .split_ascii_whitespace()
        .collect::<Vec<_>>(),
    )
    .unwrap();

    assert_eq!(info.fs_type, "xfs");
    assert_eq!(info.dev_name, "/dev/fs0");

    let info = MountInfo::new(
      LINUX_MOUNTINFO,
      &"106 109 253:6 / /mnt rw,relatime master:1 shared:2 - xfs /dev/fs0 rw"
        .split_ascii_whitespace()
        .collect::<Vec<_>>(),
    )
    .unwrap();

    assert_eq!(info.fs_type, "xfs");
    assert_eq!(info.dev_name, "/dev/fs0");
  }
}
