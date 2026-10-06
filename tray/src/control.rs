//! Service controls use the outer user bus; GUI launches retain the Plasma bus.
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceState {
    pub active: String,
    pub sub: String,
    pub detail: String,
}
impl ServiceState {
    pub fn unknown(detail: impl Into<String>) -> Self {
        Self {
            active: "unknown".into(),
            sub: String::new(),
            detail: detail.into(),
        }
    }
    pub fn parse(text: &str) -> Self {
        let values: BTreeMap<_, _> = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        if values.get("LoadState") == Some(&"not-found") {
            return Self {
                active: "missing".into(),
                sub: String::new(),
                detail: "Service is not installed".into(),
            };
        }
        let active = values
            .get("ActiveState")
            .copied()
            .unwrap_or("unknown")
            .to_owned();
        let sub = values.get("SubState").copied().unwrap_or("").to_owned();
        let detail = format!(
            "{} / {}; result={}, exit={}",
            active,
            sub,
            values.get("Result").copied().unwrap_or("unknown"),
            values.get("ExecMainStatus").copied().unwrap_or("unknown")
        );
        Self {
            active,
            sub,
            detail,
        }
    }
    pub fn running(&self) -> bool {
        matches!(self.active.as_str(), "active" | "activating" | "reloading")
    }
    pub fn label(&self) -> &str {
        match self.active.as_str() {
            "active" => "Running",
            "inactive" => "Paused / stopped",
            "failed" => "Failed",
            "activating" => "Starting",
            "deactivating" => "Stopping",
            "reloading" => "Reloading",
            "missing" => "Not installed",
            _ => "Status unavailable",
        }
    }
}

pub fn manager_environment(uid: u32) -> [(String, String); 2] {
    let runtime = format!("/run/user/{uid}");
    [
        (
            "DBUS_SESSION_BUS_ADDRESS".into(),
            format!("unix:path={runtime}/bus"),
        ),
        ("XDG_RUNTIME_DIR".into(), runtime),
    ]
}

pub fn bounded_output(command: &mut Command, timeout: Duration) -> io::Result<(bool, String)> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut text = String::new();
                if let Some(stdout) = child.stdout.take() {
                    stdout.take(8192).read_to_string(&mut text)?;
                }
                return Ok((status.success(), text));
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match result {
                    Err(error) => error,
                    _ => io::Error::new(io::ErrorKind::TimedOut, "service command timed out"),
                });
            }
        }
    }
}

pub struct Manager {
    pub uid: u32,
    pub home: PathBuf,
}
impl Manager {
    fn command(&self) -> Command {
        let mut command = Command::new("systemctl");
        command
            .args(["--user", "--no-pager"])
            .envs(manager_environment(self.uid));
        command
    }
    pub fn status(&self) -> ServiceState {
        match bounded_output(
            self.command().args([
                "show",
                "frame-voice.service",
                "--property=LoadState,ActiveState,SubState,Result,ExecMainStatus",
            ]),
            Duration::from_secs(3),
        ) {
            Ok((success, text)) => {
                let state = ServiceState::parse(&text);
                if success || state.active == "missing" {
                    state
                } else {
                    ServiceState::unknown("Service status query failed")
                }
            }
            Err(error) => ServiceState::unknown(error.to_string()),
        }
    }
    pub fn control(&self, verb: &str) -> io::Result<()> {
        if !["start", "stop", "restart"].contains(&verb) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported service control",
            ));
        }
        bounded_output(
            self.command()
                .args(["--no-block", verb, "frame-voice.service"]),
            Duration::from_secs(3),
        )
        .and_then(|(success, _)| {
            if success {
                Ok(())
            } else {
                Err(io::Error::other("Service control failed; view recent logs"))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_states_and_missing_unit_are_distinct() {
        let running = ServiceState::parse("LoadState=loaded\nActiveState=active\nSubState=running\nExecMainStatus=0\nResult=success\n");
        assert!(running.running());
        assert_eq!(running.label(), "Running");
        let failed = ServiceState::parse("LoadState=loaded\nActiveState=failed\nSubState=failed\nResult=exit-code\nExecMainStatus=1\n");
        assert!(!failed.running());
        assert_eq!(failed.label(), "Failed");
        assert_eq!(
            ServiceState::parse("LoadState=not-found\nActiveState=inactive\n").active,
            "missing"
        );
        assert_eq!(ServiceState::parse("").label(), "Status unavailable");
    }
    #[test]
    fn controls_use_outer_bus_without_changing_gui_environment() {
        assert_eq!(
            manager_environment(1000)[0].1,
            "unix:path=/run/user/1000/bus"
        );
        assert_eq!(manager_environment(1000)[1].1, "/run/user/1000");
    }
    #[test]
    fn failed_query_keeps_missing_unit_properties() {
        let (success, text) = bounded_output(
            Command::new("sh").args([
                "-c",
                "printf 'LoadState=not-found\\nActiveState=inactive\\n'; exit 4",
            ]),
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(!success);
        assert_eq!(ServiceState::parse(&text).active, "missing");
    }
    #[test]
    fn timeout_reaps_a_stalled_command() {
        let start = Instant::now();
        let error =
            bounded_output(Command::new("sleep").arg("5"), Duration::from_millis(50)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
