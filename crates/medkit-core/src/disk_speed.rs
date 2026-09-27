//! 硬盘测速（工具箱里的「硬盘测速」）：看看硬盘、U 盘实际读写有多快。验机、换了固态硬盘以后用，也能看出硬盘是不是
//! 慢得不正常。
//!
//! - 在所选的盘的根目录（测试里是临时文件夹）建一个临时文件，名字带进程号、设成隐藏；Windows 上不经过系统缓存
//!   （FILE_FLAG_NO_BUFFERING、FILE_FLAG_WRITE_THROUGH），读写的都是硬盘本身的速度；
//! - 先按顺序写最多 [`TEST_BYTES`]、再按顺序读回来，然后随机读 4 KB 的小块（打开软件、开机时就是这么读的，最能看出
//!   是不是固态硬盘）；每一步都限时，慢的 U 盘也不会等太久；
//! - 文件用 FILE_FLAG_DELETE_ON_CLOSE 打开：测完、出错，哪怕小药箱被强行结束，系统都会把它删掉；
//! - 剩余空间不到 [`MIN_FREE`] 的盘不测，也不会把盘写满；只写这一个临时文件，别的什么都不动。
//!   写 1 GB 对固态硬盘的寿命没有影响（平时每天就要写几十 GB）。
//! - 缓冲区按 4096 字节对齐，每次读写的大小也是 4096 的整数倍（不经过缓存时 Windows 的要求）。

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;

const MIB: u64 = 1024 * 1024;
/// 最多写多少
pub const TEST_BYTES: u64 = 1024 * MIB;
/// 盘上至少要剩多少空间才测（写完也还剩 1 GB）
pub const MIN_FREE: u64 = 2048 * MIB;
/// 顺序读写一次多少
const BLOCK: usize = 8 * 1024 * 1024;
/// 随机读一次多少
const SMALL: usize = 4096;
const ALIGN: usize = 4096;

/// 每一步最多多久
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub write: Duration,
    pub read: Duration,
    pub random: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self { write: Duration::from_secs(10), read: Duration::from_secs(10), random: Duration::from_secs(3) }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedResult {
    /// 顺序写、顺序读，MB/s（1 MB = 1048576 字节，和任务管理器、测速软件一样）
    pub seq_write: f64,
    pub seq_read: f64,
    /// 4 KB 随机读，MB/s 和每秒多少次
    pub random_read: f64,
    pub random_iops: f64,
    /// 实际写了多少字节（慢的盘时间到了就停）
    pub tested_bytes: u64,
    /// 大致是哪一类：nvme、sata-ssd、ssd-slow、hdd、slow
    pub verdict: &'static str,
}

/// 按 4096 字节对齐的缓冲区（不用 unsafe：多申请一点，从对齐的位置开始用）
struct Aligned {
    buf: Vec<u8>,
    offset: usize,
    len: usize,
}

impl Aligned {
    fn new(len: usize) -> Self {
        let buf = vec![0u8; len + ALIGN];
        let offset = buf.as_ptr().align_offset(ALIGN);
        Self { buf, offset, len }
    }
    fn get(&self) -> &[u8] {
        &self.buf[self.offset..self.offset + self.len]
    }
    fn get_mut(&mut self) -> &mut [u8] {
        &mut self.buf[self.offset..self.offset + self.len]
    }
}

/// 简单的伪随机数（xorshift）：填写入的数据（免得有的硬盘压缩全是 0 的数据、测得偏快）、挑随机读的位置
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn open(dir: &Path) -> std::io::Result<File> {
    let path = dir.join(format!("medkit-speed-{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_WRITE_THROUGH: u32 = 0x8000_0000;
        const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
        const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_NOT_CONTENT_INDEXED: u32 = 0x2000;
        options
            .custom_flags(FILE_FLAG_WRITE_THROUGH | FILE_FLAG_NO_BUFFERING | FILE_FLAG_DELETE_ON_CLOSE)
            .attributes(FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED)
            .share_mode(0);
    }
    let file = options.open(&path)?;
    // 别的系统上没有「关掉就删」：打开以后马上删掉名字，文件关掉时就没了
    #[cfg(not(windows))]
    std::fs::remove_file(&path)?;
    Ok(file)
}

fn mbps(bytes: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64().max(1e-6);
    (bytes as f64 / MIB as f64) / secs
}

/// 按测出来的速度大致归类（顺序读和 4 KB 随机读一起看）
pub fn verdict(seq_read: f64, random_read: f64) -> &'static str {
    if seq_read >= 700.0 {
        "nvme"
    } else if seq_read >= 250.0 {
        "sata-ssd"
    } else if random_read >= 8.0 {
        "ssd-slow"
    } else if seq_read >= 60.0 {
        "hdd"
    } else {
        "slow"
    }
}

/// 在 `dir` 里测：最多写 `max_bytes`（会取成 BLOCK 的整数倍，至少一块）。
pub fn run(dir: &Path, max_bytes: u64, limits: Limits) -> Result<SpeedResult, String> {
    let blocks = (max_bytes / BLOCK as u64).max(1);
    let mut file = open(dir).map_err(|e| format!("建不了测试用的临时文件：{e}"))?;
    let target = blocks * BLOCK as u64;
    // 先把空间占好，写的时候不用一点一点地扩大文件
    file.set_len(target).map_err(|e| format!("临时文件放不下：{e}"))?;

    let mut buf = Aligned::new(BLOCK);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ u64::from(std::process::id()));
    for chunk in buf.get_mut().chunks_exact_mut(8) {
        chunk.copy_from_slice(&rng.next().to_le_bytes());
    }

    // 顺序写
    let start = Instant::now();
    let mut written = 0u64;
    while written < target && (written == 0 || start.elapsed() < limits.write) {
        file.write_all(buf.get()).map_err(|e| format!("写入失败：{e}"))?;
        written += BLOCK as u64;
    }
    file.flush().map_err(|e| format!("写入失败：{e}"))?;
    let seq_write = mbps(written, start.elapsed());

    // 顺序读
    file.seek(SeekFrom::Start(0)).map_err(|e| format!("读取失败：{e}"))?;
    let start = Instant::now();
    let mut read = 0u64;
    while read < written && (read == 0 || start.elapsed() < limits.read) {
        file.read_exact(buf.get_mut()).map_err(|e| format!("读取失败：{e}"))?;
        read += BLOCK as u64;
    }
    let seq_read = mbps(read, start.elapsed());

    // 4 KB 随机读：只读写过的那一段
    let mut small = Aligned::new(SMALL);
    let slots = written / SMALL as u64;
    let start = Instant::now();
    let mut ops = 0u64;
    while ops == 0 || start.elapsed() < limits.random {
        let at = (rng.next() % slots) * SMALL as u64;
        file.seek(SeekFrom::Start(at)).map_err(|e| format!("读取失败：{e}"))?;
        file.read_exact(small.get_mut()).map_err(|e| format!("读取失败：{e}"))?;
        ops += 1;
    }
    let elapsed = start.elapsed().as_secs_f64().max(1e-6);
    let random_iops = ops as f64 / elapsed;
    let random_read = random_iops * SMALL as f64 / MIB as f64;

    Ok(SpeedResult {
        seq_write,
        seq_read,
        random_read,
        random_iops,
        tested_bytes: written,
        verdict: verdict(seq_read, random_read),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_measures_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let limits = Limits {
            write: Duration::from_millis(500),
            read: Duration::from_millis(500),
            random: Duration::from_millis(200),
        };
        let r = run(dir.path(), 3 * BLOCK as u64, limits).unwrap();
        assert!(r.tested_bytes >= BLOCK as u64 && r.tested_bytes <= 3 * BLOCK as u64, "{r:?}");
        assert!(r.seq_write > 0.0 && r.seq_read > 0.0 && r.random_read > 0.0 && r.random_iops > 0.0, "{r:?}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "临时文件不留");
    }

    #[test]
    fn a_tiny_size_still_writes_one_block() {
        let dir = tempfile::tempdir().unwrap();
        let quick = Limits { write: Duration::ZERO, read: Duration::ZERO, random: Duration::ZERO };
        let r = run(dir.path(), 1, quick).unwrap();
        assert_eq!(r.tested_bytes, BLOCK as u64);
    }

    #[test]
    fn verdicts_follow_the_usual_ranges() {
        assert_eq!(verdict(3200.0, 60.0), "nvme");
        assert_eq!(verdict(530.0, 35.0), "sata-ssd");
        assert_eq!(verdict(180.0, 25.0), "ssd-slow", "接在 USB 上的固态硬盘");
        assert_eq!(verdict(150.0, 0.8), "hdd");
        assert_eq!(verdict(25.0, 3.0), "slow");
    }

    #[test]
    fn buffers_are_aligned() {
        let mut a = Aligned::new(SMALL);
        assert_eq!(a.get().as_ptr() as usize % ALIGN, 0);
        assert_eq!(a.get_mut().len(), SMALL);
    }
}
