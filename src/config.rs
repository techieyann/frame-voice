use anyhow::{bail, Context, Result};
use std::{collections::BTreeMap, env, fs, path::PathBuf};
#[derive(Clone)]
pub struct Config {
    pub values: BTreeMap<String, String>,
    pub home: PathBuf,
    pub max_seconds: f64,
    pub threshold: f64,
    pub cleanup: bool,
    pub beep: bool,
    /// Max characters per paste when `VOICE_INJECT=paste`; 0 disables chunking.
    pub paste_chunk: usize,
    /// Minimum contiguous speech (ms) required before an utterance is uploaded.
    pub min_speech_ms: u64,
}
impl Config {
    pub fn load() -> Result<Self> {
        let home = PathBuf::from(env::var("HOME").context("HOME is required")?);
        let mut values = BTreeMap::new();
        // Compatibility first, new configuration second, process environment wins.
        for path in [
            home.join(".config/voice-prompt/env"),
            home.join(".config/frame-voice/env"),
        ] {
            match fs::read_to_string(&path) {
                Ok(text) => parse_env(&text, &mut values)
                    .with_context(|| format!("invalid config {}", path.display()))?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        values.extend(env::vars());
        if values.get("GROQ_API_KEY").is_none_or(|v| v.is_empty()) {
            for path in [
                ".config/frame-voice/groq.key",
                ".config/voice-prompt/groq.key",
                ".local/share/voice-prompt/groq.key",
            ] {
                if let Ok(k) = fs::read_to_string(home.join(path)) {
                    if !k.trim().is_empty() {
                        values.insert("GROQ_API_KEY".into(), k.trim().into());
                        break;
                    }
                }
            }
        }
        let mut c = Self {
            values,
            home,
            max_seconds: 30.,
            threshold: 300.,
            cleanup: true,
            beep: true,
            paste_chunk: 0,
            min_speech_ms: 200,
        };
        c.max_seconds = c
            .get("VOICE_MAX", "30")
            .parse()
            .context("VOICE_MAX must be numeric")?;
        c.threshold = c
            .get("VOICE_THRESHOLD", "300")
            .parse()
            .context("VOICE_THRESHOLD must be numeric")?;
        if !c.max_seconds.is_finite() || !(0.1..=120.).contains(&c.max_seconds) {
            bail!("VOICE_MAX must be 0.1..120 seconds");
        }
        if !c.threshold.is_finite() || !(1.0..=32768.).contains(&c.threshold) {
            bail!("VOICE_THRESHOLD must be 1..32768");
        }
        c.cleanup = c.boolean("VOICE_CLEANUP", true)?;
        c.beep = c.boolean("VOICE_BEEP", true)?;
        // Validate now so a bad value fails startup rather than a dictation.
        c.boolean("VOICE_SLASH", false)?;
        c.boolean("GROQ_USAGE_NOTIFY", true)?;
        c.paste_chunk = c
            .get("VOICE_PASTE_CHUNK", "0")
            .parse()
            .context("VOICE_PASTE_CHUNK must be numeric")?;
        if c.paste_chunk != 0 && c.paste_chunk < 40 {
            bail!("VOICE_PASTE_CHUNK must be 0 (off) or at least 40");
        }
        c.min_speech_ms = c
            .get("VOICE_MIN_SPEECH_MS", "200")
            .parse()
            .context("VOICE_MIN_SPEECH_MS must be numeric")?;
        if !(100..=5000).contains(&c.min_speech_ms) {
            bail!("VOICE_MIN_SPEECH_MS must be 100..5000 ms");
        }
        c.boolean("VOICE_NOTIFY", true)?;
        c.boolean("VRBTN_CLEAR_B_SPACE", true)?;
        for key in [
            "VOICE_WARN_REQUESTS",
            "VOICE_WARN_TOKENS",
            "VOICE_WARN_COOLDOWN_SEC",
        ] {
            c.get(key, "0")
                .parse::<i64>()
                .with_context(|| format!("{key} must be numeric"))?;
        }
        if !["groq", "local"].contains(&c.get("VOICE_BACKEND", "groq")) {
            bail!("VOICE_BACKEND must be groq or local");
        }
        for key in ["VRBTN_ARM_MS", "VRBTN_TAP_MS", "VRBTN_READY_MS"] {
            c.timing(key, 150)?;
        }
        c.asr_confidence_limits()?;
        Ok(c)
    }
    pub fn get<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.values.get(key).map(String::as_str).unwrap_or(default)
    }
    pub fn boolean(&self, key: &str, default: bool) -> Result<bool> {
        match self.values.get(key).map(|v| v.to_ascii_lowercase()) {
            None => Ok(default),
            Some(v) if ["1", "true", "yes"].contains(&v.as_str()) => Ok(true),
            Some(v) if ["0", "false", "no", ""].contains(&v.as_str()) => Ok(false),
            _ => bail!("{key} must be a boolean"),
        }
    }
    pub fn timing(&self, key: &str, default: u64) -> Result<u64> {
        let v = match self.values.get(key) {
            Some(v) => v.parse().with_context(|| format!("invalid {key}"))?,
            None => default,
        };
        if !(30..=5000).contains(&v) {
            bail!("{key} must be 30..5000 ms");
        }
        Ok(v)
    }
    pub fn asr_confidence_limits(&self) -> Result<(f64, f64)> {
        let min: f64 = self
            .get("GROQ_ASR_MIN_LOGPROB", "-0.75")
            .parse()
            .context("GROQ_ASR_MIN_LOGPROB must be numeric")?;
        let max: f64 = self
            .get("GROQ_ASR_MAX_NO_SPEECH", "0.6")
            .parse()
            .context("GROQ_ASR_MAX_NO_SPEECH must be numeric")?;
        if !min.is_finite() || !(-10.0..=0.0).contains(&min) {
            bail!("GROQ_ASR_MIN_LOGPROB must be -10..0");
        }
        if !max.is_finite() || !(0.0..=1.0).contains(&max) {
            bail!("GROQ_ASR_MAX_NO_SPEECH must be 0..1");
        }
        Ok((min, max))
    }
    pub fn bin(&self, key: &str, name: &str) -> String {
        self.values.get(key).cloned().unwrap_or_else(|| {
            let bundled = self.home.join(".local/share/frame-voice/bin").join(name);
            if bundled.is_file() {
                return bundled.to_string_lossy().into_owned();
            }
            self.home
                .join(".local/bin")
                .join(name)
                .to_string_lossy()
                .into_owned()
        })
    }
}
fn parse_env(text: &str, values: &mut BTreeMap<String, String>) -> Result<()> {
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .with_context(|| format!("line {} needs KEY=VALUE", i + 1))?;
        let key = key.trim();
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            bail!("invalid key on line {}", i + 1);
        }
        let mut value = value.trim();
        if value.starts_with(['\'', '"']) {
            let quote = value.as_bytes()[0];
            if value.len() < 2 || value.as_bytes()[value.len() - 1] != quote {
                bail!("unclosed quote on line {}", i + 1);
            }
            value = &value[1..value.len() - 1];
        }
        values.insert(key.into(), value.into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_config_and_precedence() {
        let mut m = BTreeMap::new();
        parse_env("# hi\nKEY='a=b $HOME'\n", &mut m).unwrap();
        assert_eq!(m["KEY"], "a=b $HOME");
        parse_env("KEY=new", &mut m).unwrap();
        assert_eq!(m["KEY"], "new");
        assert!(parse_env("oops", &mut m).is_err());
    }
    #[test]
    fn confidence_limits_validate_numeric_ranges_and_can_be_tuned() {
        let mut config = Config {
            home: Default::default(),
            values: BTreeMap::new(),
            max_seconds: 30.,
            threshold: 300.,
            cleanup: true,
            beep: true,
            paste_chunk: 0,
            min_speech_ms: 200,
        };
        assert_eq!(config.asr_confidence_limits().unwrap(), (-0.75, 0.6));
        for (key, bad) in [
            ("GROQ_ASR_MIN_LOGPROB", "NaN"),
            ("GROQ_ASR_MIN_LOGPROB", "0.1"),
            ("GROQ_ASR_MAX_NO_SPEECH", "inf"),
            ("GROQ_ASR_MAX_NO_SPEECH", "1.1"),
        ] {
            config.values.insert(key.into(), bad.into());
            assert!(config.asr_confidence_limits().is_err());
            config.values.clear();
        }
        config
            .values
            .insert("GROQ_ASR_MIN_LOGPROB".into(), "-1".into());
        config
            .values
            .insert("GROQ_ASR_MAX_NO_SPEECH".into(), "0.8".into());
        assert_eq!(config.asr_confidence_limits().unwrap(), (-1., 0.8));
    }
    #[test]
    fn bundled_helpers_are_preferred_and_explicit_overrides_win() {
        let home = tempfile::tempdir().unwrap();
        let mut config = Config {
            home: home.path().into(),
            values: BTreeMap::new(),
            max_seconds: 30.,
            threshold: 300.,
            cleanup: true,
            beep: true,
            paste_chunk: 0,
            min_speech_ms: 200,
        };
        assert_eq!(
            config.bin("YDOTOOL_BIN", "ydotool"),
            home.path().join(".local/bin/ydotool").to_string_lossy()
        );
        let bundled = home.path().join(".local/share/frame-voice/bin/ydotool");
        std::fs::create_dir_all(bundled.parent().unwrap()).unwrap();
        std::fs::write(&bundled, b"fixture").unwrap();
        assert_eq!(
            config.bin("YDOTOOL_BIN", "ydotool"),
            bundled.to_string_lossy()
        );
        config
            .values
            .insert("YDOTOOL_BIN".into(), "/custom/helper".into());
        assert_eq!(config.bin("YDOTOOL_BIN", "ydotool"), "/custom/helper");
    }
}
