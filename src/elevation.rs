//! Windows 管理员权限检测与 UAC 提权重启。

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::{env, io, mem::size_of};

    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };
    use windows::core::PCWSTR;

    pub(crate) fn is_elevated() -> io::Result<bool> {
        let mut token = HANDLE::default();
        // SAFETY: 打开本进程令牌只为查询提权状态；句柄在函数结束前关闭。
        unsafe {
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
                .map_err(to_io_error)?;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned_size = 0;
        // SAFETY: 传入的缓冲区与结构体大小一致，TokenElevation 写入 TOKEN_ELEVATION。
        let result = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut returned_size,
            )
            .map_err(to_io_error)
        };
        // SAFETY: token 已查询完毕，关闭句柄不影响返回值。
        unsafe {
            let _ = CloseHandle(token);
        }
        result.map(|()| elevation.TokenIsElevated != 0)
    }

    pub(crate) fn relaunch_elevated() -> io::Result<()> {
        let executable = env::current_exe()?;
        let executable = wide_string(executable.as_os_str());
        let operation = wide_string("runas");
        let arguments = wide_string(crate::single_instance::WAIT_FOR_INSTANCE_ARGUMENT);
        // SAFETY: ShellExecuteW 的字符串均为本函数构造的以 NUL 结尾的宽字符；只启动自身。
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(executable.as_ptr()),
                PCWSTR(arguments.as_ptr()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if (result.0 as usize) <= 32 {
            return Err(io::Error::from_raw_os_error(result.0 as i32));
        }
        Ok(())
    }

    fn wide_string(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt as _;
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }

    fn to_io_error(error: windows::core::Error) -> io::Error {
        io::Error::new(io::ErrorKind::Other, error.to_string())
    }
}

#[cfg(target_os = "windows")]
pub(crate) use windows_impl::{is_elevated, relaunch_elevated};

#[cfg(not(target_os = "windows"))]
pub(crate) fn is_elevated() -> std::io::Result<bool> {
    Ok(false)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn relaunch_elevated() -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "当前平台不支持管理员权限启动",
    ))
}
