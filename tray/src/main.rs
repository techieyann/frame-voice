#[cfg(any(target_os = "linux", test))]
mod control;

#[cfg(target_os = "linux")]
mod desktop {
    use super::control::{Manager, ServiceState};
    use ksni::{blocking::TrayMethods, menu::StandardItem};
    use std::{
        env, fs, io,
        path::PathBuf,
        process::{Child, Command},
        sync::mpsc::{self, SyncSender},
        time::Duration,
    };

    #[derive(Clone, Copy)]
    enum Action {
        Start,
        Stop,
        Restart,
        Config,
        Configure,
        ConfigureControllers,
        Logs,
        Folder,
        Hide,
    }
    struct Tray {
        state: ServiceState,
        message: String,
        busy: bool,
        tx: SyncSender<Action>,
        icons: String,
    }
    impl Tray {
        fn item(&self, label: &str, action: Action, enabled: bool) -> ksni::MenuItem<Self> {
            StandardItem {
                label: label.into(),
                enabled: enabled && !self.busy,
                activate: Box::new(move |tray: &mut Self| {
                    if tray.tx.try_send(action).is_ok() {
                        tray.busy = true;
                    }
                }),
                ..Default::default()
            }
            .into()
        }
    }
    impl ksni::Tray for Tray {
        const MENU_ON_ACTIVATE: bool = true;
        fn id(&self) -> String {
            "frame-voice-tray".into()
        }
        fn title(&self) -> String {
            format!("Frame Voice — {}", self.state.label())
        }
        fn icon_theme_path(&self) -> String {
            self.icons.clone()
        }
        fn icon_name(&self) -> String {
            match self.state.active.as_str() {
                "active" => "frame-voice-active",
                "inactive" => "frame-voice-stopped",
                "activating" | "deactivating" | "reloading" => "frame-voice-processing",
                _ => "frame-voice-failed",
            }
            .into()
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            // Plain, fixed-state text; do not put config contents in a tooltip.
            ksni::ToolTip {
                title: self.title(),
                description: if self.message.is_empty() {
                    self.state.detail.clone()
                } else {
                    self.message.clone()
                },
                ..Default::default()
            }
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            vec![
                StandardItem {
                    label: format!("Status: {}", self.state.label()),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                self.item(
                    "Start / resume dictation",
                    Action::Start,
                    !self.state.running(),
                ),
                self.item("Pause dictation", Action::Stop, self.state.running()),
                self.item(
                    "Restart dictation",
                    Action::Restart,
                    self.state.active != "missing",
                ),
                ksni::MenuItem::Separator,
                self.item("Configure Frame Voice…", Action::Configure, true),
                self.item("Configure controllers…", Action::ConfigureControllers, true),
                self.item("Open configuration file", Action::Config, true),
                self.item("View recent logs", Action::Logs, true),
                self.item("Open installation folder", Action::Folder, true),
                ksni::MenuItem::Separator,
                self.item("Hide tray for this Desktop session", Action::Hide, true),
            ]
        }
    }
    fn open(manager: &Manager, action: Action) -> io::Result<Child> {
        let mut command = match action {
            Action::Logs => {
                let mut command = Command::new("konsole");
                command.args([
                    "--separate",
                    "--hold",
                    "-e",
                    "journalctl",
                    "--user",
                    "-u",
                    "frame-voice.service",
                    "-n",
                    "100",
                    "-f",
                    "--no-pager",
                ]);
                command
            }
            Action::Configure => {
                let mut command = Command::new("bash");
                command.arg(manager.home.join(".local/share/frame-voice/config-gui.sh"));
                command
            }
            Action::ConfigureControllers => {
                let mut command = Command::new("bash");
                command
                    .arg(manager.home.join(".local/share/frame-voice/config-gui.sh"))
                    .arg("controllers");
                command
            }
            _ => {
                let mut command = Command::new("xdg-open");
                command.arg(manager.home.join(match action {
                    Action::Config => ".config/frame-voice/env",
                    _ => ".local/share/frame-voice",
                }));
                command
            }
        };
        // GUI commands deliberately inherit the nested Desktop environment.
        command.spawn()
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let home = PathBuf::from(env::var("HOME")?);
        let uid: u32 = fs::read_to_string("/proc/self/status")?
            .lines()
            .find_map(|line| {
                line.strip_prefix("Uid:")
                    .and_then(|s| s.split_whitespace().next())
            })
            .ok_or("cannot determine user id")?
            .parse()?;
        let manager = Manager { uid, home };
        let (tx, rx) = mpsc::sync_channel(8);
        let tray = Tray {
            state: manager.status(),
            message: String::new(),
            busy: false,
            tx,
            icons: manager
                .home
                .join(".local/share/frame-voice/tray-icons")
                .to_string_lossy()
                .into(),
        };
        // Plasma's watcher may not be ready yet; ksni also handles watcher restarts.
        let handle = tray.assume_sni_available(true).spawn()?;
        let mut windows: Vec<Child> = Vec::new();
        while !handle.is_closed() {
            let mut message = String::new();
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Action::Hide) => {
                    handle.shutdown().wait();
                    break;
                }
                Ok(
                    action @ (Action::Config
                    | Action::Configure
                    | Action::ConfigureControllers
                    | Action::Logs
                    | Action::Folder),
                ) => {
                    if windows.len() >= 8 {
                        message = "Close a log/config window before opening another".into();
                    } else if let Err(error) =
                        open(&manager, action).map(|child| windows.push(child))
                    {
                        message = format!("Could not open window: {error}");
                    }
                }
                Ok(action) => {
                    let verb = match action {
                        Action::Start => "start",
                        Action::Stop => "stop",
                        _ => "restart",
                    };
                    if let Err(error) = manager.control(verb) {
                        message = format!("Service action failed: {error}");
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            windows.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
            let state = manager.status();
            handle.update(|tray| {
                // Preserve a failed-action message until the next user action.
                if tray.busy || !message.is_empty() {
                    tray.message = message;
                }
                tray.busy = false;
                tray.state = state;
            });
        }
        // GUI viewers remain independent; reap children when they eventually close.
        for mut child in windows {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Ok(())
    }
}

fn main() {
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--build-info" || arg == "--version")
    {
        println!(
            "frame-voice-tray {}; StatusNotifier compiled={}",
            env!("CARGO_PKG_VERSION"),
            cfg!(target_os = "linux")
        );
        return;
    }
    #[cfg(target_os = "linux")]
    if let Err(error) = desktop::run() {
        eprintln!("frame-voice-tray: {error}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("frame-voice-tray requires SteamOS/Linux");
        std::process::exit(1);
    }
}
