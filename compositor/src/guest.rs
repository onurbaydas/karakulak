//! Guest session: the owner's windows are hidden and a Firefox with an empty
//! profile in memory (XDG_RUNTIME_DIR) is started with KARAKULAK_GUEST=1, so
//! karakulakd refuses files, terminal and settings. Ending it kills that
//! Firefox, deletes its profile and locks the screen: only the owner's
//! password brings their windows back.

use std::path::PathBuf;
use std::process::Child;

use smithay::desktop::Window;

use crate::State;

pub const GUEST_APP_ID: &str = "karakulak-guest";

#[derive(Default)]
pub struct Guest {
    child: Option<Child>,
    profile: Option<PathBuf>,
    /// The owner's windows while the guest uses the screens.
    hidden: Vec<Window>,
}

impl State {
    pub fn guest_active(&self) -> bool {
        self.guest.child.is_some()
    }

    /// KARAKULAK_GUEST_COMMAND starts Firefox; `{profile}` is replaced by the
    /// guest's profile folder.
    pub fn start_guest(&mut self) -> Result<(), String> {
        if self.guest_active() {
            return Ok(());
        }
        if self.lock.locked {
            return Err("the screen is locked".into());
        }
        let command = std::env::var("KARAKULAK_GUEST_COMMAND").map_err(|_| "no guest command")?;
        let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).ok_or("no runtime folder")?;
        let profile = base.join(format!("karakulak-guest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(&profile).map_err(|e| e.to_string())?;

        let windows: Vec<Window> = self.space.elements().cloned().collect();
        for window in windows {
            self.space.unmap_elem(&window);
            self.guest.hidden.push(window);
        }
        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg(command.replace("{profile}", &profile.to_string_lossy()))
            .env("KARAKULAK_GUEST", "1")
            .spawn()
            .map_err(|e| e.to_string())?;
        self.guest.child = Some(child);
        self.guest.profile = Some(profile);
        Ok(())
    }

    pub fn end_guest(&mut self) {
        let Some(mut child) = self.guest.child.take() else { return };
        let _ = child.kill();
        let _ = child.wait();
        if let Some(profile) = self.guest.profile.take() {
            let _ = std::fs::remove_dir_all(profile);
        }
        // Whatever the guest still had open goes away with its Firefox.
        let left: Vec<Window> = self.space.elements().cloned().collect();
        for window in left {
            self.space.unmap_elem(&window);
        }
        let owner = std::mem::take(&mut self.guest.hidden);
        if self.lock_available() {
            self.lock_with_hidden(owner);
        } else {
            // Development without a lock screen: just bring the windows back.
            for window in owner {
                self.place(window);
            }
        }
    }

    /// The guest's Firefox quit by itself.
    pub fn check_guest(&mut self) {
        let exited = self
            .guest
            .child
            .as_mut()
            .is_some_and(|c| c.try_wait().map(|s| s.is_some()).unwrap_or(true));
        if exited {
            self.end_guest();
        }
    }

    /// A new window while the guest session runs: the guest's own windows
    /// show, the owner's wait.
    pub fn guest_takes(&mut self, window: &Window) -> bool {
        if !self.guest_active() || crate::lock::app_id(window).as_deref() == Some(GUEST_APP_ID) {
            return false;
        }
        self.guest.hidden.push(window.clone());
        true
    }
}
