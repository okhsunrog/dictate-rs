use anyhow::{Context, Result, bail};
use nix::{
    fcntl::{Flock, FlockArg},
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use std::{
    fs::File,
    io::{Cursor, Read},
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};
const RATE: u32 = 16000;
pub const MAX_SECONDS: u64 = 600;
type Samples = Arc<Mutex<Vec<i16>>>;

/// Path of the advisory lock that guards against two Dictate instances (e.g.
/// independently launched processes) recording at the same time.
fn lock_path() -> std::path::PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join("dictate-record.lock")
}
/// Acquire the process-wide recording lock, or fail if another instance holds it.
fn acquire_record_lock() -> Result<Flock<File>> {
    let file = File::create(lock_path()).context("Не удалось открыть файл блокировки записи")?;
    Flock::lock(file, FlockArg::LockExclusiveNonblock)
        .map_err(|_| anyhow::anyhow!("Запись уже идёт в другом экземпляре Диктовки"))
}

pub struct Recording {
    child: Child,
    reader: Option<JoinHandle<Result<()>>>,
    samples: Samples,
    level: Arc<AtomicU32>,
    _lock: Flock<File>,
}
impl Recording {
    pub fn start(target: &str) -> Result<Self> {
        let lock = acquire_record_lock()?;
        let mut cmd = Command::new("pw-record");
        cmd.args([
            "--raw",
            "--rate",
            "16000",
            "--channels",
            "1",
            "--format",
            "s16",
            "--media-role",
            "Communication",
        ]);
        if !target.trim().is_empty() {
            cmd.args(["--target", target]);
        }
        cmd.arg("-")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // Guarantee the capture process dies with us even on an abrupt exit
        // (e.g. Qt.quit() skipping Rust destructors), so no orphan pw-record.
        unsafe {
            cmd.pre_exec(|| {
                nix::sys::prctl::set_pdeathsig(Signal::SIGTERM).map_err(std::io::Error::from)
            });
        }
        let mut child = cmd
            .spawn()
            .context("Не удалось запустить pw-record; установи pipewire-audio")?;
        let mut stdout = child.stdout.take().unwrap();
        let samples: Samples = Arc::new(Mutex::new(Vec::new()));
        let level = Arc::new(AtomicU32::new(0));
        let data = samples.clone();
        let meter = level.clone();
        let reader = std::thread::spawn(move || {
            let mut buffer = [0u8; 3200];
            let mut pending = None;
            loop {
                let count = stdout.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                let mut decoded = Vec::with_capacity(count / 2 + 1);
                let mut index = 0;
                if let Some(low) = pending.take() {
                    decoded.push(i16::from_le_bytes([low, buffer[0]]));
                    index = 1;
                }
                while index + 1 < count {
                    decoded.push(i16::from_le_bytes([buffer[index], buffer[index + 1]]));
                    index += 2;
                }
                if index < count {
                    pending = Some(buffer[index]);
                }
                let rms = (decoded
                    .iter()
                    .map(|s| (*s as f32 / 32768.0).powi(2))
                    .sum::<f32>()
                    / decoded.len().max(1) as f32)
                    .sqrt();
                meter.store(rms.to_bits(), Ordering::Relaxed);
                let mut samples = data.lock().unwrap();
                let remaining = RATE as usize * MAX_SECONDS as usize - samples.len();
                samples.extend(decoded.into_iter().take(remaining));
                if remaining == 0 {
                    break;
                }
            }
            Ok(())
        });
        Ok(Self {
            child,
            reader: Some(reader),
            samples,
            level,
            _lock: lock,
        })
    }
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
    pub fn has_exited(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
    fn stop_child(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = kill(Pid::from_raw(self.child.id() as i32), Signal::SIGINT);
            for _ in 0..50 {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
    pub fn finish(mut self) -> Result<Vec<u8>> {
        self.stop_child();
        self.reader
            .take()
            .unwrap()
            .join()
            .map_err(|_| anyhow::anyhow!("Ошибка потока записи"))??;
        let samples = self.samples.lock().unwrap();
        if samples.len() < RATE as usize / 4 {
            bail!("Запись слишком короткая или микрофон недоступен");
        }
        encode_wav(&samples)
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        self.stop_child();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
impl crate::engine::Recorder for Recording {
    fn level(&self) -> f32 {
        Recording::level(self)
    }
    fn has_exited(&mut self) -> bool {
        Recording::has_exited(self).unwrap_or(true)
    }
    fn finish(self: Box<Self>) -> Result<Vec<u8>> {
        Recording::finish(*self)
    }
}
fn encode_wav(samples: &[i16]) -> Result<Vec<u8>> {
    let mut data = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut data,
        hound::WavSpec {
            channels: 1,
            sample_rate: RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    for sample in samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(data.into_inner())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wav_round_trip() {
        let samples = [0, -32000, 32000, -1];
        let bytes = encode_wav(&samples).unwrap();
        let mut r = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(r.spec().sample_rate, 16000);
        assert_eq!(
            r.samples::<i16>().collect::<Result<Vec<_>, _>>().unwrap(),
            samples
        );
    }
}
