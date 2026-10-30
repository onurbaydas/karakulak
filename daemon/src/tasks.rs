//! Task manager: system load from /proc, the user's programs, and stopping
//! one that hangs. Only the user's own programs are listed or stopped.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::time::Duration;

/// How long CPU use is measured over.
const SAMPLE: Duration = Duration::from_millis(400);

/// (busy, total) jiffies of all CPUs together.
fn cpu_times() -> Option<(u64, u64)> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let fields: Vec<u64> = stat.lines().next()?.split_whitespace().skip(1).filter_map(|f| f.parse().ok()).collect();
    let total: u64 = fields.iter().sum();
    // idle and iowait
    let idle = fields.get(3)? + fields.get(4).unwrap_or(&0);
    Some((total - idle, total))
}

fn meminfo() -> HashMap<String, u64> {
    fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            Some((name.to_string(), rest.split_whitespace().next()?.parse::<u64>().ok()? * 1024))
        })
        .collect()
}

fn disk(path: &str) -> Option<(u64, u64)> {
    let c_path = std::ffi::CString::new(path).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = stat.f_frsize as u64;
    Some(((stat.f_blocks - stat.f_bfree) as u64 * block, stat.f_blocks as u64 * block))
}

struct Process {
    pid: u32,
    name: String,
    /// utime + stime in jiffies.
    cpu: u64,
    memory: u64,
}

/// The current user's processes.
fn processes() -> Vec<Process> {
    let uid = unsafe { libc::getuid() };
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
    let mut list = Vec::new();
    for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else { continue };
        let dir = entry.path();
        let owner = fs::read_to_string(dir.join("status")).ok().and_then(|s| {
            s.lines().find_map(|l| l.strip_prefix("Uid:")?.split_whitespace().next()?.parse::<u32>().ok())
        });
        if owner != Some(uid) {
            continue;
        }
        let Ok(stat) = fs::read_to_string(dir.join("stat")) else { continue };
        // The name is in parentheses and may itself contain spaces.
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else { continue };
        let name = stat[open + 1..close].to_string();
        let fields: Vec<&str> = stat[close + 2..].split_whitespace().collect();
        let number = |i: usize| fields.get(i).and_then(|f| f.parse::<u64>().ok()).unwrap_or(0);
        // Fields after the name start at "state" (3rd field of the line).
        let cpu = number(11) + number(12);
        let memory = number(21) * page;
        list.push(Process { pid, name, cpu, memory });
    }
    list
}

/// `{"cpu", "memory": {"used", "total"}, "disk": {"used", "total"},
///   "processes": [{"pid", "name", "cpu", "memory"}]}`, heaviest first.
pub fn overview() -> Result<Value, String> {
    let before_cpu = cpu_times().ok_or("cannot read /proc/stat")?;
    let before: HashMap<u32, u64> = processes().into_iter().map(|p| (p.pid, p.cpu)).collect();
    std::thread::sleep(SAMPLE);
    let after_cpu = cpu_times().ok_or("cannot read /proc/stat")?;
    let elapsed = (after_cpu.1 - before_cpu.1).max(1) as f64;
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;

    let mut list: Vec<Value> = processes()
        .into_iter()
        .map(|p| {
            let used = p.cpu.saturating_sub(*before.get(&p.pid).unwrap_or(&p.cpu)) as f64;
            // Share of one CPU, like `top`.
            json!({ "pid": p.pid, "name": p.name, "cpu": used / elapsed * cpus, "memory": p.memory })
        })
        .collect();
    list.sort_by(|a, b| b["memory"].as_u64().cmp(&a["memory"].as_u64()));

    let mem = meminfo();
    let total = mem.get("MemTotal").copied().unwrap_or(0);
    let available = mem.get("MemAvailable").copied().unwrap_or(0);
    let (disk_used, disk_total) = disk("/").unwrap_or((0, 0));
    Ok(json!({
        "cpu": (after_cpu.0 - before_cpu.0) as f64 / elapsed,
        "memory": { "used": total.saturating_sub(available), "total": total },
        "disk": { "used": disk_used, "total": disk_total },
        "processes": list,
    }))
}

/// Asks a program to quit; `force` kills it.
pub fn stop(pid: u64, force: bool) -> Result<Value, String> {
    let pid = i32::try_from(pid).map_err(|_| "invalid pid")?;
    if pid <= 1 || pid as u32 == std::process::id() {
        return Err("not this one".into());
    }
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    // The kernel refuses other users' programs; the list only has ours anyway.
    if unsafe { libc::kill(pid, signal) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(Value::Null)
}
