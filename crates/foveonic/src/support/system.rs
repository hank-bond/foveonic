//! Collects static system capacity off the UI thread without recording unrelated host information.

use serde_json::{json, Value};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

pub fn collect() -> Value {
    let system = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing())
            .with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    let mut context = json!({"status":"basic inventory collected", "os":{
        "family":std::env::consts::OS, "product":System::name(),
        "version":System::os_version(), "description":System::long_os_version(),
        "kernel_or_build":System::kernel_version(), "architecture":std::env::consts::ARCH},
        "cpu":{"models":system.cpus().iter().map(|cpu|cpu.brand()).collect::<std::collections::BTreeSet<_>>(),
            "logical_cores":system.cpus().len(), "physical_cores":System::physical_core_count()},
        "total_ram_bytes":system.total_memory(),
        "gpu_inventory":{"status":"pending"}});
    super::update_context("system", context.clone());
    #[cfg(target_os = "windows")]
    {
        context["os"]["windows_update"] = windows_update();
    }
    context["gpu_inventory"] = gpu_inventory(system.total_memory());
    context["status"] = json!("collected");
    context
}

#[cfg(target_os = "windows")]
fn windows_update() -> Value {
    use windows::{
        core::w,
        Win32::System::Registry::{
            RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
        },
    };
    let mut revision = 0u32;
    let mut bytes = std::mem::size_of_val(&revision) as u32;
    // The fixed-size buffers stay live during these read-only registry calls.
    let revision_status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("UBR"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut revision as *mut u32).cast()),
            Some(&mut bytes),
        )
    };
    let mut version = [0u16; 128];
    let mut bytes = std::mem::size_of_val(&version) as u32;
    let version_status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("DisplayVersion"),
            RRF_RT_REG_SZ,
            None,
            Some(version.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    let length = version
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(version.len());
    json!({"update_build_revision":if revision_status.0==0 {Some(revision)}else{None},
        "display_version":if version_status.0==0 {Some(String::from_utf16_lossy(&version[..length]))}else{None},
        "revision_query_status":revision_status.0,"display_version_query_status":version_status.0,
        "source":"HKLM Windows NT/CurrentVersion"})
}

#[cfg(target_os = "windows")]
fn gpu_inventory(_: u64) -> Value {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    let collect = || -> windows::core::Result<Vec<Value>> {
        // DXGI reports physical adapter capacity separately from API resource limits.
        // COM interface values own their references for the entire enumeration.
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1()? };
        let mut records = Vec::new();
        for index in 0..64 {
            let adapter = match unsafe { factory.EnumAdapters1(index) } {
                Ok(adapter) => adapter,
                Err(error)
                    if error.code() == windows::Win32::Graphics::Dxgi::DXGI_ERROR_NOT_FOUND =>
                {
                    break
                }
                Err(error) => return Err(error),
            };
            let desc = unsafe { adapter.GetDesc1()? };
            let length = desc
                .Description
                .iter()
                .position(|&v| v == 0)
                .unwrap_or(desc.Description.len());
            records.push(json!({"name":String::from_utf16_lossy(&desc.Description[..length]),
                "vendor":desc.VendorId,"device":desc.DeviceId,"source":"DXGI_ADAPTER_DESC1",
                "dedicated_video_memory_bytes":desc.DedicatedVideoMemory,
                "dedicated_system_memory_bytes":desc.DedicatedSystemMemory,
                "shared_system_memory_limit_bytes":desc.SharedSystemMemory,
                "capacity_note":"Shared system memory is a DXGI sharing limit, not dedicated VRAM or current availability.",
                "software_adapter":desc.Flags & 2 != 0}));
        }
        Ok(records)
    };
    match collect() {
        Ok(records) => json!({"status":"collected","adapters":records}),
        Err(error) => {
            json!({"status":"unavailable","reason":error.to_string(),"hresult":error.code().0})
        }
    }
}

#[cfg(target_os = "macos")]
fn gpu_inventory(total_ram: u64) -> Value {
    match display_report() {
        Ok(report) => mac_gpu_inventory(&report, total_ram, cfg!(target_arch = "aarch64")),
        Err(error) => json!({"status":"unavailable","reason":error.to_string()}),
    }
}

#[cfg(any(target_os = "macos", test))]
fn mac_gpu_inventory(report: &Value, total_ram: u64, apple_silicon: bool) -> Value {
    let Some(adapters) = report.get("SPDisplaysDataType").and_then(Value::as_array) else {
        return json!({"status":"unavailable","reason":"system_profiler omitted GPU inventory"});
    };
    let adapters:Vec<_>=adapters.iter().map(|gpu| {
        let unified=apple_silicon && gpu.get("spdisplays_vendor").and_then(Value::as_str)==Some("sppci_vendor_Apple");
        json!({"name":gpu.get("sppci_model").or_else(||gpu.get("_name")),
            "vendor_description":gpu.get("spdisplays_vendor"),"device_description":gpu.get("spdisplays_device-id"),
            "source":"system_profiler SPDisplaysDataType", "gpu_cores":gpu.get("sppci_cores"),
            "dedicated_video_memory_description":gpu.get("spdisplays_vram").or_else(||gpu.get("sppci_vram")),
            "unified_memory":unified,"unified_physical_pool_bytes":if unified {Some(total_ram)}else{None},
            "capacity_note":if unified {"CPU and GPU share this physical RAM pool. There is no separate fixed VRAM capacity."} else {"Dedicated capacity is unavailable if no VRAM description was reported."}})
    }).collect();
    json!({"status":"collected","adapters":adapters})
}

#[cfg(target_os = "macos")]
fn display_report() -> std::io::Result<Value> {
    use std::{
        io::{Read, Seek},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut file = tempfile::tempfile()?;
    let mut child = Command::new("/usr/sbin/system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .stdout(Stdio::from(file.try_clone()?))
        .stderr(Stdio::null())
        .spawn()?;
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if began.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "GPU inventory timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "GPU inventory exited with {status}"
        )));
    }
    file.rewind()?;
    let mut output = String::new();
    file.take(512 * 1024).read_to_string(&mut output)?;
    serde_json::from_str(&output).map_err(std::io::Error::other)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn gpu_inventory(_: u64) -> Value {
    json!({"status":"unavailable","reason":"Physical GPU capacity collection is not implemented for this platform. Selected adapter identity is recorded separately."})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mac_inventory_distinguishes_unified_memory_and_omits_display_serials() {
        let input = json!({"SPDisplaysDataType":[{"sppci_model":"Apple M1 Pro","spdisplays_vendor":"sppci_vendor_Apple","spdisplays_ndrvs":[{"serial":"private-display-serial"}]}]});
        let output = mac_gpu_inventory(&input, 32 << 30, true);
        assert_eq!(
            output["adapters"][0]["unified_physical_pool_bytes"],
            32u64 << 30
        );
        assert!(output["adapters"][0]["dedicated_video_memory_description"].is_null());
        assert!(!output.to_string().contains("private-display-serial"));
        let discrete = mac_gpu_inventory(
            &json!({"SPDisplaysDataType":[{"sppci_model":"AMD Radeon","sppci_vram":"8 GB"}]}),
            16 << 30,
            false,
        );
        assert_eq!(
            discrete["adapters"][0]["dedicated_video_memory_description"],
            "8 GB"
        );
        assert!(discrete["adapters"][0]["unified_physical_pool_bytes"].is_null());
    }
}
