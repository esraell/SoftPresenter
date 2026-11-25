//! What a report says about the computer, read once by the reporter's own thread: the system,
//! the processor and the memory. Reading them runs small programs (`sw_vers`, `sysctl`,
//! PowerShell), which is why it is never done on a thread that draws. The computer's own name
//! and its model are never read.

use super::sample::quiet_command;

/// The computer as it was when the app started.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hardware {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub cpu_model: String,
    pub cores: usize,
    pub ram_mb: u64,
}

impl Hardware {
    pub fn read() -> Hardware {
        let mut hardware = Hardware {
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            cores: std::thread::available_parallelism().map_or(1, usize::from),
            ..Hardware::default()
        };
        if cfg!(target_os = "macos") {
            let read = |program: &str, args: &[&str]| quiet_command(program, args).map(|text| text.trim().to_owned());
            hardware.os_version = read("sw_vers", &["-productVersion"]).unwrap_or_default();
            hardware.cpu_model = read("sysctl", &["-n", "machdep.cpu.brand_string"]).unwrap_or_default();
            hardware.ram_mb = read("sysctl", &["-n", "hw.memsize"])
                .and_then(|bytes| bytes.parse::<u64>().ok())
                .map_or(0, |bytes| bytes / 1_048_576);
        } else if cfg!(windows) {
            // One call to PowerShell for all of it: slow (a second), so done once, here.
            const SCRIPT: &str = "$o=Get-CimInstance Win32_OperatingSystem;$c=Get-CimInstance Win32_Processor|Select-Object -First 1;$s=Get-CimInstance Win32_ComputerSystem;\
                'os='+$o.Caption;'version='+$o.Version;'build='+$o.BuildNumber;'cpu='+$c.Name;'ram='+$s.TotalPhysicalMemory";
            if let Some(text) = quiet_command("powershell", &["-NoProfile", "-NonInteractive", "-Command", SCRIPT]) {
                hardware.apply_windows(&text);
            }
            if hardware.cpu_model.is_empty() {
                hardware.cpu_model = std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default();
            }
        } else if let Ok(text) = std::fs::read_to_string("/etc/os-release") {
            hardware.apply_os_release(&text);
            hardware.cpu_model = std::fs::read_to_string("/proc/cpuinfo")
                .ok()
                .and_then(|text| {
                    text.lines()
                        .find(|line| line.starts_with("model name"))
                        .and_then(|line| line.split(':').nth(1))
                        .map(|name| name.trim().to_owned())
                })
                .unwrap_or_default();
            hardware.ram_mb = std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|text| {
                    text.lines()
                        .find(|line| line.starts_with("MemTotal:"))
                        .and_then(|line| line.split_whitespace().nth(1)?.parse::<u64>().ok())
                })
                .map_or(0, |kb| kb / 1024);
        }
        hardware
    }

    /// What the PowerShell call above printed: `key=value` lines.
    pub fn apply_windows(&mut self, text: &str) {
        let (mut caption, mut build) = (String::new(), String::new());
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once('=') else { continue };
            let value = value.trim();
            match key {
                "os" => caption = value.to_owned(),
                "version" => self.os_version = value.to_owned(),
                "build" => build = value.to_owned(),
                "cpu" => self.cpu_model = value.to_owned(),
                "ram" => self.ram_mb = value.parse::<u64>().map_or(0, |bytes| bytes / 1_048_576),
                _ => {}
            }
        }
        // Windows 11 still says 10.0; its build number (22000 and over) tells which it is.
        if !caption.is_empty() {
            self.os_version = format!("{} (build {build})", caption.trim_start_matches("Microsoft ").trim());
        }
    }

    pub fn apply_os_release(&mut self, text: &str) {
        for line in text.lines() {
            if let Some(name) = line.strip_prefix("PRETTY_NAME=") {
                self.os_version = name.trim_matches('"').to_owned();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_powershell_prints_becomes_the_windows_facts() {
        let mut hardware = Hardware::default();
        hardware.apply_windows(
            "os=Microsoft Windows 11 Pro\nversion=10.0.22631\nbuild=22631\ncpu=Intel(R) Core(TM) i5-12400\nram=17048502272\n",
        );
        assert_eq!(hardware.os_version, "Windows 11 Pro (build 22631)");
        assert_eq!((hardware.cpu_model.as_str(), hardware.ram_mb), ("Intel(R) Core(TM) i5-12400", 16_258));
    }

    #[test]
    fn linux_names_itself_in_os_release() {
        let mut hardware = Hardware::default();
        hardware.apply_os_release("NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n");
        assert_eq!(hardware.os_version, "Ubuntu 24.04 LTS");
    }
}
