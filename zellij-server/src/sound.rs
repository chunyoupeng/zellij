use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use zellij_utils::consts::ZELLIJ_CACHE_DIR;

static LAST_SOUND_PLAYED_MILLIS: AtomicU64 = AtomicU64::new(0);
const SOUND_DEBOUNCE_MILLIS: u64 = 250;

/// Synthesize a clean 2-tone "ding-dong" chime as 16-bit PCM WAV.
pub fn generate_ding_dong_wav() -> Vec<u8> {
    let sample_rate = 22050u32;
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    let total_samples = (sample_rate as f32 * 0.6) as usize; // 600ms
    let mut pcm_data = Vec::with_capacity(total_samples * 2);

    let ding_freq = 784.0f32; // G5
    let dong_freq = 523.25f32; // C5
    let ding_samples = (sample_rate as f32 * 0.18) as usize; // 180ms

    for i in 0..total_samples {
        let (freq, t, decay_rate) = if i < ding_samples {
            let t = i as f32 / sample_rate as f32;
            (ding_freq, t, 12.0f32)
        } else {
            let t = (i - ding_samples) as f32 / sample_rate as f32;
            (dong_freq, t, 8.0f32)
        };
        let envelope = (-decay_rate * t).exp();
        // fundamental + overtone harmonics for a chime/bell timbre
        let sample_f = ((2.0 * std::f32::consts::PI * freq * t).sin()
            + 0.3 * (2.0 * std::f32::consts::PI * freq * 2.0 * t).sin()
            + 0.15 * (2.0 * std::f32::consts::PI * freq * 3.0 * t).sin())
            * envelope
            * 0.7;
        let sample_i = (sample_f.clamp(-1.0, 1.0) * 32767.0) as i16;
        pcm_data.extend_from_slice(&sample_i.to_le_bytes());
    }

    let mut wav = Vec::with_capacity(44 + pcm_data.len());
    // RIFF header
    wav.extend_from_slice(b"RIFF");
    let file_size_minus_8 = (36 + pcm_data.len()) as u32;
    wav.extend_from_slice(&file_size_minus_8.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size 16
    wav.extend_from_slice(&1u16.to_le_bytes());  // PCM format
    wav.extend_from_slice(&num_channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * num_channels as u32 * (bits_per_sample / 8) as u32;
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = num_channels * (bits_per_sample / 8);
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm_data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm_data);
    wav
}

/// Ensures the default synthesized sound file is written to cache.
fn ensure_default_sound_file() -> Option<PathBuf> {
    let cache_dir = &*ZELLIJ_CACHE_DIR;
    let _ = std::fs::create_dir_all(cache_dir);
    let sound_path = cache_dir.join("agent_completion_ding_dong.wav");
    if !sound_path.exists() {
        let wav_bytes = generate_ding_dong_wav();
        if std::fs::write(&sound_path, wav_bytes).is_err() {
            return None;
        }
    }
    Some(sound_path)
}

/// Resolve sound path from configured path (or macOS sound name) or fallback to default.
fn resolve_sound_path(custom_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(custom) = custom_path {
        if custom.exists() {
            return Some(custom.to_path_buf());
        }
        #[cfg(target_os = "macos")]
        {
            if let Some(sound_name) = custom.to_str() {
                let mac_sound = PathBuf::from(format!("/System/Library/Sounds/{}.aiff", sound_name));
                if mac_sound.exists() {
                    return Some(mac_sound);
                }
            }
        }
        log::warn!("Custom agent completion sound path does not exist: {:?}", custom);
        None
    } else {
        ensure_default_sound_file()
    }
}

/// Play audio asynchronously using the available system player.
pub fn play_completion_sound(custom_path: Option<&Path>) {
    let now_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let last = LAST_SOUND_PLAYED_MILLIS.load(Ordering::Relaxed);
    if now_millis.saturating_sub(last) < SOUND_DEBOUNCE_MILLIS {
        return;
    }
    LAST_SOUND_PLAYED_MILLIS.store(now_millis, Ordering::Relaxed);

    let Some(sound_path) = resolve_sound_path(custom_path) else {
        return;
    };

    std::thread::spawn(move || {
        play_file(&sound_path);
    });
}

fn play_file(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("afplay").arg(path).spawn().and_then(|mut c| c.wait());
    }
    #[cfg(target_os = "linux")]
    {
        let players = ["pw-play", "paplay", "aplay", "play", "canberra-gtk-play"];
        for player in players {
            if let Ok(mut child) = Command::new(player).arg(path).spawn() {
                let _ = child.wait();
                return;
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "(New-Object Media.SoundPlayer '{}').PlaySync()",
            path.to_string_lossy().replace('\'', "''")
        );
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .spawn()
            .and_then(|mut c| c.wait());
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let players = ["pw-play", "paplay", "aplay", "play"];
        for player in players {
            if let Ok(mut child) = Command::new(player).arg(path).spawn() {
                let _ = child.wait();
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ding_dong_wav() {
        let wav = generate_ding_dong_wav();
        assert!(wav.len() > 44);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn test_resolve_sound_path_default() {
        let path = resolve_sound_path(None);
        assert!(path.is_some());
    }

    #[test]
    fn test_resolve_sound_path_nonexistent() {
        let path = resolve_sound_path(Some(Path::new("/nonexistent_sound_file_123.wav")));
        assert!(path.is_none());
    }
}
