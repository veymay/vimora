//! Rooms: tabs, creating/renaming/deleting rooms and assigning computers to them.

use super::*;
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

impl ConsoleApp {
    pub(super) fn in_tab(&self, v: &View) -> bool {
        match &self.room_tab {
            RoomTab::All => true,
            RoomTab::Unassigned => v.room.is_none(),
            RoomTab::Room(r) => v.room.as_deref() == Some(r.as_str()),
        }
    }

    pub(super) fn assign(&mut self, keys: &[String], room: Option<String>) {
        let mut c = self.cfg.write().unwrap();
        for k in keys {
            match &room {
                Some(r) => {
                    c.assignments.insert(k.clone(), r.clone());
                }
                None => {
                    c.assignments.remove(k);
                }
            }
        }
        config::save(&c);
    }

    pub(super) fn delete_room(&mut self, name: &str) {
        let mut c = self.cfg.write().unwrap();
        c.rooms.retain(|r| r != name);
        c.assignments.retain(|_, r| r != name);
        config::save(&c);
        drop(c);
        if self.room_tab == RoomTab::Room(name.to_string()) {
            self.room_tab = RoomTab::All;
        }
    }

    /// Right-clicking a selected card acts on the whole selection.
    pub(super) fn assign_keys(&self, id: u64, all: &[View]) -> Vec<String> {
        let ids: Vec<u64> = if self.selected.contains(&id) {
            self.selected.iter().copied().collect()
        } else {
            vec![id]
        };
        all.iter()
            .filter(|v| ids.contains(&v.id))
            .map(|v| v.key.clone())
            .collect()
    }

    pub(super) fn save_room(&mut self, name: String, rename: Option<String>) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        {
            let mut c = self.cfg.write().unwrap();
            if c.rooms
                .iter()
                .any(|r| r.eq_ignore_ascii_case(&name) && Some(r) != rename.as_ref())
            {
                drop(c);
                self.hub.lock().unwrap().log(
                    Level::Warn,
                    tf!("\"{name}\" nomli xona allaqachon bor", name = name),
                );
                return;
            }
            match &rename {
                Some(old) => {
                    if let Some(r) = c.rooms.iter_mut().find(|r| *r == old) {
                        *r = name.clone();
                    }
                    for v in c.assignments.values_mut() {
                        if v == old {
                            *v = name.clone();
                        }
                    }
                }
                None => c.rooms.push(name.clone()),
            }
        }
        let keys = std::mem::take(&mut self.pending_assign);
        if !keys.is_empty() {
            self.assign(&keys, Some(name.clone()));
        }
        self.room_tab = RoomTab::Room(name);
        config::save(&self.cfg.read().unwrap());
    }
}
