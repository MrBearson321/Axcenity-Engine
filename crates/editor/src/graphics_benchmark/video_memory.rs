//! How much memory the graphics card has, where the system says without
//! unsafe code (wgpu doesn't report it): on Windows the display driver's
//! `HardwareInformation.qwMemorySize` in the registry, read with `reg`; on
//! Linux the amdgpu driver's `mem_info_vram_total`. Elsewhere, and for
//! cards that don't say, `None`, and the benchmark keeps the preset's
//! texture budget.

/// The card's own memory in bytes, matched by its PCI vendor and device
/// (`wgpu::AdapterInfo::vendor` and `device`). Slow (it may start a
/// program): call it off the UI thread.
pub fn read(vendor: u32, device: u32) -> Option<u64> {
    if vendor == 0 && device == 0 {
        return None;
    }
    #[cfg(windows)]
    {
        windows(vendor, device)
    }
    #[cfg(target_os = "linux")]
    {
        linux(vendor, device)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

#[cfg(windows)]
fn windows(vendor: u32, device: u32) -> Option<u64> {
    use std::os::windows::process::CommandExt;
    /// Display adapters' driver keys.
    const DISPLAY_CLASS: &str =
        r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    /// No console window flashing up.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let query = |value: &str| {
        std::process::Command::new("reg")
            .args(["query", DISPLAY_CLASS, "/s", "/v", value])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let sizes = query("HardwareInformation.qwMemorySize")?;
    let ids = query("MatchingDeviceId")?;
    from_registry(&ids, &sizes, vendor, device)
}

#[cfg(any(windows, test))]
/// Each key's value in `reg query /s /v` output: the key's line, then its
/// value's line (`name  TYPE  data`).
fn registry_values(output: &str) -> Vec<(&str, &str)> {
    let mut values = Vec::new();
    let mut key = None;
    for line in output.lines() {
        if line.starts_with("HKEY_") {
            key = Some(line.trim());
        } else if let Some(key) = key
            && let Some(data) = line.split_whitespace().nth(2)
        {
            values.push((key, data));
        }
    }
    values
}

#[cfg(any(windows, test))]
/// The memory size of the display driver key whose `MatchingDeviceId`
/// names this PCI vendor and device, from two `reg query` outputs.
fn from_registry(ids: &str, sizes: &str, vendor: u32, device: u32) -> Option<u64> {
    let wanted = format!("ven_{vendor:04x}&dev_{device:04x}");
    let sizes = registry_values(sizes);
    registry_values(ids)
        .into_iter()
        .filter(|(_, id)| id.to_ascii_lowercase().contains(&wanted))
        .find_map(|(key, _)| {
            let (_, size) = sizes.iter().find(|(other, _)| *other == key)?;
            u64::from_str_radix(size.trim_start_matches("0x"), 16)
                .ok()
                .filter(|&bytes| bytes > 0)
        })
}

#[cfg(target_os = "linux")]
fn linux(vendor: u32, device: u32) -> Option<u64> {
    let read_hex = |path: &std::path::Path| {
        let text = std::fs::read_to_string(path).ok()?;
        u32::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
    };
    std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .find_map(|card| {
            let folder = card.path().join("device");
            (read_hex(&folder.join("vendor"))? == vendor
                && read_hex(&folder.join("device"))? == device)
                .then_some(())?;
            std::fs::read_to_string(folder.join("mem_info_vram_total"))
                .ok()?
                .trim()
                .parse()
                .ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: &str = r"
HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000
    MatchingDeviceId    REG_SZ    pci\ven_1002&dev_7550&rev_c0

HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0001
    MatchingDeviceId    REG_SZ    PCI\VEN_1002&DEV_13C0

End of search: 2 match(es) found.
";
    const SIZES: &str = r"
HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000
    HardwareInformation.qwMemorySize    REG_QWORD    0x3f0000000

HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0001
    HardwareInformation.qwMemorySize    REG_QWORD    0x20000000

End of search: 2 match(es) found.
";

    #[test]
    fn the_registry_names_each_cards_memory() {
        // An RX 9070 XT (16,128 MiB) and an integrated GPU (512 MiB).
        assert_eq!(
            from_registry(IDS, SIZES, 0x1002, 0x7550),
            Some(0x0003_f000_0000)
        );
        assert_eq!(from_registry(IDS, SIZES, 0x1002, 0x13c0), Some(512 << 20));
        assert_eq!(from_registry(IDS, SIZES, 0x10de, 0x2684), None);
        assert_eq!(from_registry("", SIZES, 0x1002, 0x7550), None);
    }

    #[test]
    fn an_adapter_without_ids_reads_nothing() {
        assert_eq!(read(0, 0), None);
    }
}
