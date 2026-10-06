use crate::{
    audio::{self, Control, Process},
    config::Config,
    speech::{self, Output},
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{atomic::Ordering, OnceLock},
    time::{Duration, Instant},
};
fn request(c: &Config, path: &str, body: Vec<u8>, content_type: &str) -> Result<String> {
    let key = c.get("GROQ_API_KEY", "");
    if key.is_empty() {
        bail!("GROQ_API_KEY is missing");
    }
    let base = c
        .get("GROQ_API_BASE", "https://api.groq.com/openai/v1")
        .trim_end_matches('/');
    if !base.starts_with("https://") {
        bail!("GROQ_API_BASE must use HTTPS");
    }
    // Retain DNS/TLS state and keep-alive connections across ASR and cleanup.
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    let agent = AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(45)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into()
    });
    // Never include server response bodies or request credentials in error logs.
    let mut response = agent
        .post(format!("{base}{path}"))
        .header("Authorization", format!("Bearer {key}"))
        .header("User-Agent", "frame-voice/0.1")
        .header("Content-Type", content_type)
        .send(body)
        .map_err(|_| anyhow::anyhow!("Groq request failed (network or timeout)"))?;
    crate::usage::observe(c, response.headers());
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        if status == 429 {
            crate::usage::rate_limited(c);
        }
        bail!("Groq request returned HTTP {status}");
    }
    response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .context("read Groq response")
}
fn multipart(fields: &[(&str, &str)], wav: &[u8], boundary: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(wav.len() + 1024);
    for (name, value) in fields {
        write!(
            body,
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
        .unwrap();
    }
    write!(body,"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\n").unwrap();
    body.extend(wav);
    write!(body, "\r\n--{boundary}--\r\n").unwrap();
    body
}
fn groq_language(language: &str) -> &str {
    match language.trim() {
        "auto" | "" => "",
        language => language,
    }
}
fn local_language(language: &str) -> &str {
    match language.trim() {
        "auto" | "" => "auto",
        language => language,
    }
}
fn segment_uncertain(segment: &Value, min_logprob: f64, max_no_speech: f64) -> bool {
    segment
        .get("avg_logprob")
        .and_then(Value::as_f64)
        .is_some_and(|p| p < min_logprob)
        || segment
            .get("no_speech_prob")
            .and_then(Value::as_f64)
            .is_some_and(|p| p > max_no_speech)
}
fn rejected_transcription(result: &Value, min_logprob: f64, max_no_speech: f64) -> bool {
    // Confidence is a decoding score, not a calibrated accuracy percentage.
    // Cancel only if every returned segment reports weak decoding or non-speech.
    // Missing metadata retains the local audio gate rather than rejecting speech.
    result
        .get("segments")
        .and_then(Value::as_array)
        .is_some_and(|segments| {
            !segments.is_empty()
                && segments
                    .iter()
                    .all(|segment| segment_uncertain(segment, min_logprob, max_no_speech))
        })
}
fn transcribe(c: &Config, pcm: &[i16], control: &Control) -> Result<String> {
    let wav = audio::wav(pcm);
    if c.get("VOICE_BACKEND", "groq") == "groq" {
        // A random boundary, independent of audio or user-provided prompt contents.
        let tmp = tempfile::Builder::new().prefix("fv-boundary-").tempfile()?;
        let boundary = format!(
            "framevoice{}",
            tmp.path().file_name().unwrap().to_string_lossy()
        );
        let mut fields = vec![
            ("model", c.get("GROQ_ASR_MODEL", "whisper-large-v3-turbo")),
            ("response_format", "verbose_json"),
            ("temperature", "0"),
        ];
        for (name, value) in [
            ("language", groq_language(c.get("VOICE_LANG", "en"))),
            ("prompt", c.get("VOICE_PROMPT_TEXT", "")),
        ] {
            if !value.is_empty() {
                fields.push((name, value));
            }
        }
        let body = multipart(&fields, &wav, &boundary);
        let result: Value = serde_json::from_str(&request(
            c,
            "/audio/transcriptions",
            body,
            &format!("multipart/form-data; boundary={boundary}"),
        )?)?;
        let (min_logprob, max_no_speech) = c.asr_confidence_limits()?;
        let rejected = rejected_transcription(&result, min_logprob, max_no_speech);
        if c.boolean("VRBTN_DEBUG", false)? {
            if let Some(segments) = result.get("segments").and_then(Value::as_array) {
                for segment in segments {
                    eprintln!(
                        "asr confidence: avg_logprob={:?}; no_speech_prob={:?}; uncertain={}",
                        segment.get("avg_logprob").and_then(Value::as_f64),
                        segment.get("no_speech_prob").and_then(Value::as_f64),
                        segment_uncertain(segment, min_logprob, max_no_speech)
                    );
                }
            }
            eprintln!("asr gate: rejected={rejected}");
        }
        if rejected {
            return Ok(String::new());
        }
        return result["text"]
            .as_str()
            .map(str::to_owned)
            .context("ASR response has no text");
    }
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(&wav)?;
    let output = tempfile::tempfile()?;
    let model = c.values.get("VOICE_MODEL").cloned().unwrap_or_else(|| {
        c.home
            .join(".local/share/whisper/models/ggml-base.en.bin")
            .to_string_lossy()
            .into_owned()
    });
    let mut cmd = Command::new(c.bin("WHISPER_BIN", "whisper-cli"));
    cmd.args(["-m", &model, "-f"]).arg(file.path()).args([
        "-l",
        local_language(c.get("VOICE_LANG", "en")),
        "-nt",
        "-np",
        "-t",
        c.get("VOICE_THREADS", "4"),
    ]);
    if !c.get("VOICE_PROMPT_TEXT", "").is_empty() {
        cmd.args(["--prompt", c.get("VOICE_PROMPT_TEXT", "")]);
    }
    let mut child = Process(
        cmd.stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(Stdio::null())
            .spawn()
            .context("start whisper-cli")?,
    );
    let start = Instant::now();
    loop {
        if control.canceled() {
            bail!("canceled");
        }
        if let Some(status) = child.0.try_wait()? {
            if !status.success() {
                bail!("whisper-cli failed");
            }
            break;
        }
        if start.elapsed() > Duration::from_secs(120) {
            bail!("whisper-cli timed out");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    use std::io::{Read, Seek, SeekFrom};
    let mut output = output;
    output.seek(SeekFrom::Start(0))?;
    let mut text = String::new();
    output.take(1024 * 1024).read_to_string(&mut text)?;
    Ok(text)
}
fn cleanup(c: &Config, text: &str) -> Result<String> {
    // Dictation cleanup. `VOICE_SLASH` additionally renders the spoken word
    // "slash" as the character '/', so agents can be driven with skill commands.
    let mut system = cleanup_prompt(c)?;
    if c.boolean("VOICE_SLASH", false)? {
        system.push_str(
            " Render the spoken words \"slash\" and \"forward slash\" as '/' only when the speaker names that character.",
        );
    }
    system.push_str(" Return only cleaned text, no preamble or quotes.");
    let payload = json!({"model":c.get("GROQ_LLM_MODEL","openai/gpt-oss-20b"),"temperature":0,"messages":[
        {"role":"system","content":system},
        {"role":"user","content":text}]});
    let result: Value = serde_json::from_str(&request(
        c,
        "/chat/completions",
        serde_json::to_vec(&payload)?,
        "application/json",
    )?)?;
    let cleaned = result["choices"][0]["message"]["content"]
        .as_str()
        .context("cleanup response has no content")?
        .trim();
    if cleaned.is_empty() {
        bail!("empty cleanup response");
    }
    Ok(cleaned.into())
}
fn cleanup_prompt(c: &Config) -> Result<String> {
    let path = c
        .values
        .get("GROQ_PROMPT_FILE")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| c.home.join(".config/frame-voice/groq-prompt.txt"));
    let prompt = if path.exists() {
        std::fs::read_to_string(path).context("read transcript cleanup prompt")?
    } else {
        include_str!("../assets/groq-prompt.txt").into()
    };
    if prompt.trim().is_empty() || prompt.len() > 32768 {
        bail!("cleanup prompt must contain 1–32768 bytes");
    }
    Ok(prompt)
}
pub fn run(c: &Config, control: &Control, record_only: Option<&std::path::Path>) -> Result<Output> {
    let pcm = audio::record(c, control)?;
    if control.canceled() {
        return Ok(Output::Cancel);
    }
    if let Some(path) = record_only {
        std::fs::write(path, audio::wav(&pcm))?;
        return Ok(Output::Cancel);
    }
    let trimmed = audio::trim(&pcm, c.threshold, c.min_speech_ms);
    let voice_evidence =
        trimmed.and_then(|_| crate::vad::contains_speech(&pcm, c.threshold, c.min_speech_ms));
    let accepted = trimmed.is_some() && voice_evidence != Some(false);
    if c.boolean("VRBTN_DEBUG", false)? {
        eprintln!(
            "audio gate: {} ms; rms={:.0}; speech={}; vad={:?}",
            pcm.len() * 1000 / audio::RATE as usize,
            audio::rms(&pcm),
            accepted,
            voice_evidence
        );
    }
    if !accepted {
        return Ok(Output::Cancel);
    }
    let Some(pcm) = trimmed else {
        return Ok(Output::Cancel);
    };
    // Speech confirmed: tell the daemon so it can play the stop tone and show
    // the processing icon together. Silence never reaches here, so it still
    // only ever gets the daemon's cancel cue.
    control.voiced.store(true, Ordering::SeqCst);
    let text = transcribe(c, pcm, control)?;
    if control.canceled() {
        return Ok(Output::Cancel);
    }
    let text = speech::strip_artifacts(&text);
    let raw = speech::classify(&text);
    let Output::Text(text) = raw else {
        return Ok(raw);
    };
    if text.is_empty() {
        return Ok(Output::Cancel);
    }
    let text = if c.cleanup && c.get("VOICE_BACKEND", "groq") == "groq" {
        match cleanup(c, &text) {
            Ok(text) => text,
            Err(_) => {
                eprintln!("cleanup unavailable; using raw transcription");
                text
            }
        }
    } else {
        text
    };
    if control.canceled() {
        return Ok(Output::Cancel);
    }
    // Cleanup is text only. It can never introduce a new submit/clear command.
    Ok(Output::Text(text))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_detection_uses_each_backends_contract() {
        assert_eq!(groq_language("auto"), "");
        assert_eq!(groq_language(""), "");
        assert_eq!(groq_language("fr"), "fr");
        assert_eq!(local_language("auto"), "auto");
        assert_eq!(local_language(""), "auto");
        assert_eq!(local_language("ja"), "ja");
    }
    #[test]
    fn confidence_rejects_uncertain_noise_and_preserves_confident_short_speech() {
        for text in ["Yeah", "you.", "Thank you."] {
            assert!(rejected_transcription(
                &json!({"text":text,
                "segments":[{"no_speech_prob":0.95,"avg_logprob":-0.2}]}),
                -0.75,
                0.6
            ));
            assert!(rejected_transcription(
                &json!({"text":text,
                "segments":[{"no_speech_prob":0.05,"avg_logprob":-0.85}]}),
                -0.75,
                0.6
            ));
            assert!(!rejected_transcription(
                &json!({"text":text,
                "segments":[{"no_speech_prob":0.05,"avg_logprob":-0.2}]}),
                -0.75,
                0.6
            ));
        }
        assert!(!rejected_transcription(&json!({"text":"Yeah"}), -0.75, 0.6));
        assert!(!rejected_transcription(
            &json!({"segments":[
            {"no_speech_prob":0.9,"avg_logprob":-1.5},
            {"no_speech_prob":0.01,"avg_logprob":-0.1}]}),
            -0.75,
            0.6
        ));
        assert!(!rejected_transcription(
            &json!({"segments":[
            {"no_speech_prob":0.05,"avg_logprob":-0.85}]}),
            -1.,
            0.6
        ));
    }
    #[test]
    fn cleanup_prompt_is_neutral_and_customized_only_from_user_config() {
        let directory = tempfile::tempdir().unwrap();
        let c = Config {
            home: directory.path().into(),
            values: Default::default(),
            max_seconds: 30.,
            threshold: 300.,
            cleanup: true,
            beep: true,
            paste_chunk: 0,
            min_speech_ms: 200,
        };
        let default = cleanup_prompt(&c).unwrap();
        assert!(default.contains("Never translate"));
        assert!(!default.contains("commit"));
        assert!(!default.contains("mchq"));
        let path = directory.path().join(".config/frame-voice/groq-prompt.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "Custom terminology. Preserve language.").unwrap();
        assert_eq!(
            cleanup_prompt(&c).unwrap(),
            "Custom terminology. Preserve language."
        );
    }
    #[test]
    fn multipart_framing_preserves_wave_bytes() {
        let body = multipart(&[("model", "test")], &[0, 255, 13, 10], "boundary");
        assert!(body.ends_with(b"\r\n--boundary--\r\n"));
        assert!(body.windows(4).any(|w| w == [0, 255, 13, 10]));
    }
}
