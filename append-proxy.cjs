const fs = require('fs');
const path = 'C:\\Users\\Алексей\\.minimax-agent\\projects\\singbox-client\\src-tauri\\src\\process.rs';
const add = `

// --- System proxy management (Windows only) -----------------------
// When sing-box is in system_proxy mode we have to also tell Windows
// to send HTTP/HTTPS traffic through 127.0.0.1:<port>. Without this,
// the browser etc. go straight to the internet and the proxy has
// nothing to forward.
//
// We use the WinINET registry keys under HKCU and broadcast a
// WM_SETTINGCHANGE so most apps pick it up immediately.

#[cfg(windows)]
pub fn apply_system_proxy(host: &str, port: u16) -> AppResult<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let proxy = format!("{host}:{port}");
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let settings = hkcu
        .open_subkey_with_flags(
            "Software\\\\Microsoft\\\\Windows\\\\CurrentVersion\\\\Internet Settings",
            KEY_SET_VALUE,
        )
        .map_err(|e| AppError::Spawn(format!("open Internet Settings: {e}")))?;
    settings
        .set_value("ProxyEnable", &1u32)
        .map_err(|e| AppError::Spawn(format!("set ProxyEnable: {e}")))?;
    settings
        .set_value("ProxyServer", &proxy)
        .map_err(|e| AppError::Spawn(format!("set ProxyServer: {e}")))?;
    Ok(())
}

#[cfg(windows)]
pub fn clear_system_proxy() -> AppResult<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let settings = hkcu
        .open_subkey_with_flags(
            "Software\\\\Microsoft\\\\Windows\\\\CurrentVersion\\\\Internet Settings",
            KEY_SET_VALUE,
        )
        .map_err(|e| AppError::Spawn(format!("open Internet Settings: {e}")))?;
    settings
        .set_value("ProxyEnable", &0u32)
        .map_err(|e| AppError::Spawn(format!("clear ProxyEnable: {e}")))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn apply_system_proxy(_host: &str, _port: u16) -> AppResult<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn clear_system_proxy() -> AppResult<()> {
    Ok(())
}
`;
const cur = fs.readFileSync(path, 'utf-8');
fs.writeFileSync(path, cur + '\n' + add);
console.log('appended', add.length, 'bytes');
