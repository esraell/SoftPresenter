//! How much processor time and memory the app, its FFmpeg children and the whole computer use,
//! read every few seconds by the reporter's own thread. Everything here is cheap (one run of
//! `ps` on a Mac, a few reads of `/proc` on Linux, three calls on Windows) and never runs on the
//! thread that draws.
//!
//! The systems give processor time as seconds used since a program began, so two readings and
//! the time between them give a percentage. A percentage is of one processor: 100 is one whole
//! core, so the app can show 250. The whole computer's is of all its cores, 0 to 100.

use std::collections::HashMap;
use std::time::Instant;

/// What a process is to us.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    App,
    /// Started by the app, or by one the app started (FFmpeg).
    Kid,
    /// Someone else's (`ps` lists every process; Windows only asks about the app's own).
    #[cfg_attr(windows, allow(dead_code))]
    Other,
    /// A reading that is already the whole computer's (Windows gives it so).
    #[cfg_attr(not(windows), allow(dead_code))]
    Whole,
}

/// One process in a reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    pub pid: u32,
    /// Processor seconds used since it began.
    pub cpu_s: f64,
    pub mb: f64,
    pub role: Role,
}

/// All that was read at one moment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    pub procs: Vec<Proc>,
    pub free_mb: Option<f64>,
}

/// What a reading says, with the time before it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Sample {
    /// Percent of one processor.
    pub app_cpu: f32,
    pub kids_cpu: f32,
    /// Percent of all the processors.
    pub system_cpu: f32,
    pub app_mb: f32,
    pub kids_mb: f32,
    pub free_mb: Option<f32>,
}

use serde::Serialize;

/// Reads and remembers the last reading, to turn the next into percentages.
pub struct Sampler {
    before: Option<(Instant, HashMap<u32, f64>)>,
    cores: f64,
    /// Once a minute, the figure the system calls the app's footprint (a Mac's).
    footprint: Option<f32>,
}

impl Sampler {
    pub fn new() -> Sampler {
        let cores = std::thread::available_parallelism().map_or(1, usize::from) as f64;
        Sampler { before: None, cores, footprint: None }
    }

    /// Takes a reading now. The first call has nothing to compare with and gives zero percentages.
    pub fn sample(&mut self) -> Option<Sample> {
        let reading = read()?;
        Some(self.compare(Instant::now(), &reading))
    }

    /// Asks the system for the app's footprint, which counts memory the graphics card shares
    /// (on a Mac, what Activity Monitor shows). It is slower than a sample, so it is asked once a
    /// minute.
    pub fn ask_footprint(&mut self) {
        self.footprint = footprint_mb();
    }

    fn compare(&mut self, now: Instant, reading: &Reading) -> Sample {
        let mut now_cpu = HashMap::with_capacity(reading.procs.len());
        let (mut app, mut kids, mut all, mut whole) = (0.0, 0.0, 0.0, None::<f64>);
        let (mut app_mb, mut kids_mb) = (0.0, 0.0);
        let before = self.before.take();
        let seconds = before.as_ref().map_or(0.0, |(then, _)| now.duration_since(*then).as_secs_f64());
        for proc in &reading.procs {
            now_cpu.insert(proc.pid, proc.cpu_s);
            // A process not seen before has used its time since it began, within this interval.
            let was = before.as_ref().and_then(|(_, map)| map.get(&proc.pid)).copied();
            let used = match (&before, was) {
                (None, _) => 0.0,
                (Some(_), Some(was)) => (proc.cpu_s - was).max(0.0),
                (Some(_), None) => proc.cpu_s.min(seconds),
            };
            match proc.role {
                Role::App => {
                    app += used;
                    app_mb += proc.mb;
                    all += used;
                }
                Role::Kid => {
                    kids += used;
                    kids_mb += proc.mb;
                    all += used;
                }
                Role::Other => all += used,
                Role::Whole => whole = Some(used),
            }
        }
        self.before = Some((now, now_cpu));
        let per = |used: f64| if seconds > 0.0 { (used / seconds * 100.0) as f32 } else { 0.0 };
        let system = whole.unwrap_or(all);
        Sample {
            app_cpu: per(app),
            kids_cpu: per(kids),
            system_cpu: (per(system) / self.cores as f32).min(100.0),
            app_mb: self.footprint.unwrap_or(app_mb as f32),
            kids_mb: kids_mb as f32,
            free_mb: reading.free_mb.map(|mb| mb as f32),
        }
    }
}

/// Processor time as `ps` writes it: `0:01.23`, `1:02:03.45` or `2-03:04:05`.
#[cfg(any(target_os = "macos", test))]
pub fn parse_cpu_time(text: &str) -> Option<f64> {
    let (days, rest) = match text.split_once('-') {
        Some((days, rest)) => (days.parse::<f64>().ok()?, rest),
        None => (0.0, text),
    };
    let mut seconds = 0.0;
    for part in rest.split(':') {
        seconds = seconds * 60.0 + part.parse::<f64>().ok()?;
    }
    Some(days * 86_400.0 + seconds)
}

/// What `ps -A -o pid=,ppid=,cputime=,rss=,comm=` printed, with this process's own children (and
/// theirs) marked as kids, less the programs this sampler runs to look.
#[cfg(any(target_os = "macos", test))]
pub fn parse_ps(text: &str, me: u32) -> Reading {
    let mut rows: Vec<(u32, u32, f64, f64, &str)> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(pid), Some(ppid), Some(cpu), Some(rss)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let name = line.trim_start().splitn(5, char::is_whitespace).last().unwrap_or("").trim();
        let (Ok(pid), Ok(ppid), Some(cpu), Ok(rss)) =
            (pid.parse::<u32>(), ppid.parse::<u32>(), parse_cpu_time(cpu), rss.parse::<f64>())
        else {
            continue;
        };
        rows.push((pid, ppid, cpu, rss / 1024.0, name));
    }
    let mut family = vec![me];
    let mut grew = true;
    while grew {
        grew = false;
        for (pid, ppid, ..) in &rows {
            if family.contains(ppid) && !family.contains(pid) {
                family.push(*pid);
                grew = true;
            }
        }
    }
    let ours = |name: &str| {
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        matches!(base, "ps" | "vm_stat" | "footprint" | "pmset" | "sysctl")
    };
    let procs = rows
        .into_iter()
        .map(|(pid, _, cpu_s, mb, name)| {
            let role = if pid == me {
                Role::App
            } else if family.contains(&pid) && !ours(name) {
                Role::Kid
            } else {
                Role::Other
            };
            Proc { pid, cpu_s, mb, role }
        })
        .collect();
    Reading { procs, free_mb: None }
}

/// The memory a Mac has free to give: pages free, inactive and speculative, from `vm_stat`.
#[cfg(any(target_os = "macos", test))]
pub fn parse_vm_stat(text: &str) -> Option<f64> {
    let page: f64 = text.lines().next()?.split("page size of ").nth(1)?.split_whitespace().next()?.parse().ok()?;
    let pages = |name: &str| -> f64 {
        text.lines()
            .find(|line| line.starts_with(name))
            .and_then(|line| line.rsplit(':').next())
            .and_then(|value| value.trim().trim_end_matches('.').parse().ok())
            .unwrap_or(0.0)
    };
    let free = pages("Pages free") + pages("Pages inactive") + pages("Pages speculative");
    Some(free * page / 1_048_576.0)
}

/// `Footprint: 1234 MB` (or KB, GB) from the line `footprint` prints.
#[cfg(any(target_os = "macos", test))]
pub fn parse_footprint(text: &str) -> Option<f32> {
    let line = text.lines().find(|line| line.contains("Footprint:"))?;
    let mut words = line.split("Footprint:").nth(1)?.split_whitespace();
    let amount: f32 = words.next()?.parse().ok()?;
    match words.next()? {
        "KB" => Some(amount / 1024.0),
        "MB" => Some(amount),
        "GB" => Some(amount * 1024.0),
        _ => None,
    }
}

/// Memory available on Linux, from `/proc/meminfo`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_meminfo(text: &str) -> Option<f64> {
    let line = text.lines().find(|line| line.starts_with("MemAvailable:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

/// Processor seconds (user and system) and resident megabytes from `/proc/<pid>/stat`, with the
/// parent's pid. The name in the second field is in brackets and may hold spaces and brackets.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_proc_stat(text: &str, clock_ticks: f64, page_bytes: f64) -> Option<(u32, f64, f64)> {
    let after = &text[text.rfind(')')? + 1..];
    let fields: Vec<&str> = after.split_whitespace().collect();
    // After the name: state, ppid, ..., utime is the 12th field, stime the 13th, rss the 22nd.
    let ppid: u32 = fields.get(1)?.parse().ok()?;
    let used: f64 = fields.get(11)?.parse::<f64>().ok()? + fields.get(12)?.parse::<f64>().ok()?;
    let rss: f64 = fields.get(21)?.parse().ok()?;
    Some((ppid, used / clock_ticks, rss * page_bytes / 1_048_576.0))
}

/// Takes a reading with the system's own tools.
fn read() -> Option<Reading> {
    #[cfg(windows)]
    {
        let stats = sp_winapi::process_stats()?;
        let mb = |bytes: u64| bytes as f64 / 1_048_576.0;
        // The three pids are made up: only that a program is the same one each time matters.
        Some(Reading {
            procs: vec![
                Proc {
                    pid: std::process::id(),
                    cpu_s: stats.app_cpu_s,
                    mb: mb(stats.app_private_bytes),
                    role: Role::App,
                },
                Proc { pid: u32::MAX, cpu_s: stats.kids_cpu_s, mb: mb(stats.kids_private_bytes), role: Role::Kid },
                Proc { pid: u32::MAX - 1, cpu_s: stats.system_busy_s.unwrap_or(0.0), mb: 0.0, role: Role::Whole },
            ],
            free_mb: stats.free_bytes.map(mb),
        })
    }
    #[cfg(target_os = "macos")]
    {
        let out = quiet_command("ps", &["-A", "-o", "pid=,ppid=,cputime=,rss=,comm="])?;
        let mut reading = parse_ps(&out, std::process::id());
        reading.free_mb = quiet_command("vm_stat", &[]).and_then(|text| parse_vm_stat(&text));
        Some(reading)
    }
    #[cfg(target_os = "linux")]
    {
        let me = std::process::id();
        let mut rows: Vec<(u32, u32, f64, f64)> = Vec::new();
        for entry in std::fs::read_dir("/proc").ok()?.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()) else { continue };
            let Ok(text) = std::fs::read_to_string(entry.path().join("stat")) else { continue };
            if let Some((ppid, cpu, mb)) = parse_proc_stat(&text, 100.0, 4096.0) {
                rows.push((pid, ppid, cpu, mb));
            }
        }
        let mut family = vec![me];
        let mut grew = true;
        while grew {
            grew = false;
            for (pid, ppid, ..) in &rows {
                if family.contains(ppid) && !family.contains(pid) {
                    family.push(*pid);
                    grew = true;
                }
            }
        }
        let procs = rows
            .into_iter()
            .map(|(pid, _, cpu_s, mb)| Proc {
                pid,
                cpu_s,
                mb,
                role: if pid == me {
                    Role::App
                } else if family.contains(&pid) {
                    Role::Kid
                } else {
                    Role::Other
                },
            })
            .collect();
        let free_mb = std::fs::read_to_string("/proc/meminfo").ok().and_then(|text| parse_meminfo(&text));
        Some(Reading { procs, free_mb })
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Runs a program to read what it prints, without a console window on Windows.
pub fn quiet_command(program: &str, args: &[&str]) -> Option<String> {
    let mut command = std::process::Command::new(program);
    command.args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let out = command.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A Mac's name for what the app really holds, GPU memory included.
fn footprint_mb() -> Option<f32> {
    #[cfg(target_os = "macos")]
    {
        let pid = std::process::id().to_string();
        quiet_command("footprint", &["-p", &pid]).and_then(|text| parse_footprint(&text))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn processor_time_is_read_in_every_form_ps_writes_it() {
        assert_eq!(parse_cpu_time("0:01.50"), Some(1.5));
        assert_eq!(parse_cpu_time("1:02:03.00"), Some(3723.0));
        assert_eq!(parse_cpu_time("2-03:04:05"), Some(2.0 * 86_400.0 + 3.0 * 3600.0 + 245.0));
        assert_eq!(parse_cpu_time("nope"), None);
    }

    const PS: &str = "\
    1     0   0:10.00   2048 /sbin/launchd
  500     1   1:00.00 204800 /Applications/SoftPresenter.app/Contents/MacOS/softpresenter
  501   500   0:30.00  51200 /Applications/SoftPresenter.app/Contents/MacOS/ffmpeg
  502   501   0:01.00   1024 /usr/bin/helper tool
  503   500   0:00.10    900 /bin/ps
  900     1   5:00.00 512000 /Applications/Some Miner.app/Contents/MacOS/miner
";

    #[test]
    fn the_app_its_ffmpeg_and_the_rest_are_told_apart() {
        let reading = parse_ps(PS, 500);
        let role = |pid: u32| reading.procs.iter().find(|proc| proc.pid == pid).map(|proc| proc.role);
        assert_eq!(role(500), Some(Role::App));
        assert_eq!(role(501), Some(Role::Kid));
        assert_eq!(role(502), Some(Role::Kid), "a child's child");
        assert_eq!(role(503), Some(Role::Other), "the sampler's own ps is not counted as FFmpeg");
        assert_eq!(role(900), Some(Role::Other));
        let app = reading.procs.iter().find(|proc| proc.pid == 500).unwrap();
        assert_eq!((app.cpu_s, app.mb), (60.0, 200.0));
    }

    #[test]
    fn two_readings_give_percentages_of_a_core_and_of_the_computer() {
        let mut sampler = Sampler { before: None, cores: 4.0, footprint: None };
        let start = Instant::now();
        let first = parse_ps(PS, 500);
        let zero = sampler.compare(start, &first);
        assert_eq!((zero.app_cpu, zero.kids_cpu, zero.system_cpu), (0.0, 0.0, 0.0), "nothing to compare with yet");
        // Ten seconds later the app used 5 s, ffmpeg 20 s, the miner 10 s, a new program 2 s.
        let mut later = first.clone();
        for proc in &mut later.procs {
            proc.cpu_s += match proc.pid {
                500 => 5.0,
                501 => 20.0,
                900 => 10.0,
                _ => 0.0,
            };
        }
        later.procs.retain(|proc| proc.pid != 1);
        later.procs.push(Proc { pid: 777, cpu_s: 2.0, mb: 5.0, role: Role::Other });
        later.free_mb = Some(2048.0);
        let sample = sampler.compare(start + Duration::from_secs(10), &later);
        assert_eq!(sample.app_cpu.round(), 50.0);
        assert_eq!(sample.kids_cpu.round(), 200.0, "two cores' worth, in percent of one");
        // 5 + 20 + 10 + 2 = 37 s of 40 core-seconds.
        assert_eq!(sample.system_cpu.round(), 93.0);
        assert_eq!((sample.app_mb, sample.free_mb), (200.0, Some(2048.0)));
    }

    #[test]
    fn a_whole_computer_figure_is_used_as_it_comes() {
        let mut sampler = Sampler { before: None, cores: 2.0, footprint: None };
        let start = Instant::now();
        let mut reading = Reading {
            procs: vec![
                Proc { pid: 1, cpu_s: 10.0, mb: 100.0, role: Role::App },
                Proc { pid: 2, cpu_s: 100.0, mb: 0.0, role: Role::Whole },
            ],
            free_mb: None,
        };
        sampler.compare(start, &reading);
        reading.procs[0].cpu_s += 1.0;
        reading.procs[1].cpu_s += 10.0;
        let sample = sampler.compare(start + Duration::from_secs(10), &reading);
        assert_eq!((sample.app_cpu.round(), sample.system_cpu.round()), (10.0, 50.0));
    }

    #[test]
    fn what_the_systems_print_is_read() {
        let vm = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:                               1000.\nPages active:                             5000.\nPages inactive:                           3000.\nPages speculative:                        1000.\n";
        assert_eq!(parse_vm_stat(vm), Some(5000.0 * 16384.0 / 1_048_576.0));
        assert_eq!(
            parse_footprint("SoftPresenter [500]: 64-bit\n    Footprint: 1536 MB (16384 bytes per page)\n"),
            Some(1536.0)
        );
        assert_eq!(parse_footprint("Footprint: 2 GB"), Some(2048.0));
        assert_eq!(parse_meminfo("MemTotal: 100 kB\nMemAvailable:    2097152 kB\n"), Some(2048.0));
        let stat =
            "500 (soft presenter) (x)) S 1 500 500 0 -1 4194560 100 0 0 0 300 200 0 0 20 0 8 0 100 1000000 25600 0";
        assert_eq!(parse_proc_stat(stat, 100.0, 4096.0), Some((1, 5.0, 100.0)));
    }
}
