use std::fs::{self, OpenOptions};
use std::path::Path;

pub fn directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    #[cfg(windows)]
    restrict_windows_directory(path)?;
    Ok(())
}

pub fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

#[cfg(windows)]
fn restrict_windows_directory(path: &Path) -> Result<(), String> {
    use std::os::windows::{ffi::OsStrExt, process::CommandExt};
    use std::process::Command;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
    };

    static SID: OnceLock<Result<String, String>> = OnceLock::new();
    let sid = SID
        .get_or_init(|| {
            let output = Command::new("whoami.exe")
                .args(["/user", "/fo", "csv", "/nh"])
                .creation_flags(0x0800_0000)
                .output()
                .map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&output.stdout);
            let sid = text
                .trim()
                .rsplit(',')
                .next()
                .unwrap_or("")
                .trim_matches('"');
            if !output.status.success()
                || !sid.starts_with("S-1-")
                || !sid
                    .bytes()
                    .all(|c| c.is_ascii_digit() || c == b'-' || c == b'S')
            {
                return Err("Cannot determine the current Windows user SID.".into());
            }
            Ok(sid.to_owned())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    // Replace the DACL with one inheritable grant, preserving the owner.
    // Private keys created below this directory inherit the same restriction.
    let descriptor: Vec<u16> = format!("D:P(A;OICI;FA;;;{sid})")
        .encode_utf16()
        .chain([0])
        .collect();
    let path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut security = std::ptr::null_mut();
    // Both APIs receive NUL-terminated strings; LocalFree releases the descriptor.
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor.as_ptr(),
            1,
            &mut security,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let result = SetFileSecurityW(
            path.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            security,
        );
        let error = (result == 0).then(std::io::Error::last_os_error);
        LocalFree(security);
        if let Some(error) = error {
            return Err(error.to_string());
        }
    }
    Ok(())
}
