use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Duration,
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask},
        Event,
    },
};
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Focus {
    pub display: String,
    pub window: u32,
    pub class: String,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub revision: u64,
    pub focus: Option<Focus>,
}
#[derive(Default)]
struct State {
    displays: BTreeMap<String, Focus>,
    revision: u64,
}
#[derive(Clone, Default)]
pub struct Context(Arc<(Mutex<State>, Condvar)>);
impl Context {
    pub fn start() -> Self {
        let context = Self::default();
        let c = context.clone();
        thread::spawn(move || {
            let running = Arc::new(Mutex::new(BTreeSet::new()));
            loop {
                for display in discover() {
                    if !running.lock().unwrap().insert(display.clone()) {
                        continue;
                    }
                    let c = c.clone();
                    let running = running.clone();
                    thread::spawn(move || {
                        let _ = watch(&display, &c);
                        c.set(&display, None); // Disconnect must close the gate, not retain stale focus.
                        running.lock().unwrap().remove(&display);
                    });
                }
                // Discovery/reconnect only; X11 focus itself is event-driven.
                thread::sleep(Duration::from_secs(2));
            }
        });
        context
    }
    fn set(&self, display: &str, focus: Option<Focus>) {
        let mut s = self.0 .0.lock().unwrap();
        if s.displays.get(display) == focus.as_ref() {
            return;
        }
        match focus {
            Some(f) => {
                s.displays.insert(display.into(), f);
            }
            None => {
                s.displays.remove(display);
            }
        }
        s.revision = s.revision.wrapping_add(1);
        self.0 .1.notify_all();
    }
    pub fn wait_for_change(&self, revision: u64) {
        let state = self.0 .0.lock().unwrap();
        let _ = self
            .0
             .1
            .wait_timeout_while(state, Duration::from_secs(1), |s| s.revision == revision);
    }
    /// Read only the generation when no owned display/class strings are needed.
    pub fn revision(&self) -> u64 {
        self.0 .0.lock().unwrap().revision
    }
    pub fn matches(&self, snapshot: &Snapshot) -> bool {
        let s = self.0 .0.lock().unwrap();
        s.revision == snapshot.revision
            && s.displays.len() == 1
            && s.displays.values().next() == snapshot.focus.as_ref()
    }
    pub fn snapshot(&self) -> Snapshot {
        let s = self.0 .0.lock().unwrap();
        // Ambiguous focus is unsafe for typing/clearing; wait for it to settle.
        Snapshot {
            revision: s.revision,
            focus: if s.displays.len() == 1 {
                s.displays.values().next().cloned()
            } else {
                None
            },
        }
    }
}
fn discover() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/tmp/.X11-unix") {
        for entry in rd.flatten() {
            let n = entry.file_name();
            let n = n.to_string_lossy();
            if let Some(num) = n.strip_prefix('X') {
                if !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) {
                    out.push(format!(":{num}"));
                }
            }
        }
    }
    out.sort();
    out
}
fn watch(display: &str, context: &Context) -> anyhow::Result<()> {
    let (conn, screen) = x11rb::connect(Some(display))?;
    let root = conn.setup().roots[screen].root;
    let atom = conn
        .intern_atom(false, b"GAMESCOPE_FOCUSED_WINDOW")?
        .reply()?
        .atom;
    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?
    .check()?;
    conn.flush()?;
    let refresh = || -> anyhow::Result<()> {
        let r = conn
            .get_property(false, root, atom, AtomEnum::CARDINAL, 0, 1)?
            .reply()?;
        let window = r.value32().and_then(|mut v| v.next()).unwrap_or(0);
        let focus = if window == 0 {
            None
        } else {
            let class = conn
                .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)?
                .reply()
                .ok()
                .map(|p| String::from_utf8_lossy(&p.value).replace('\0', "/"))
                .unwrap_or_default();
            Some(Focus {
                display: display.into(),
                window,
                class,
            })
        };
        context.set(display, focus);
        Ok(())
    };
    refresh()?;
    loop {
        if let Event::PropertyNotify(e) = conn.wait_for_event()? {
            if e.window == root && e.atom == atom {
                refresh()?;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disconnect_ambiguity_and_return_invalidate() {
        let c = Context::default();
        let f = Focus {
            display: ":0".into(),
            window: 10,
            class: "Konsole".into(),
        };
        c.set(":0", Some(f.clone()));
        let first = c.snapshot();
        assert!(first.focus.is_some());
        assert_eq!(c.revision(), first.revision);
        assert!(c.matches(&first));
        c.set(
            ":1",
            Some(Focus {
                display: ":1".into(),
                ..f.clone()
            }),
        );
        assert!(c.snapshot().focus.is_none());
        assert!(!c.matches(&first));
        c.set(":1", None);
        assert!(c.snapshot().revision > first.revision);
        assert!(!c.matches(&first)); // Returning to the same app cannot revive an old job.
        assert!(c.matches(&c.snapshot()));
        c.set(":0", None);
        assert!(c.snapshot().focus.is_none());
    }
}
