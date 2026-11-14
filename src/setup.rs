//! Setup Menu (`seg_004f`):
//! - Video Mode (VGA 256 Color / EGA 16 Color)
//! - Music, Sound Effects, Puzzle Timer, Grid Lines, Speech Text
//! - Save & Return (`mjp_config.pre`)

use std::path::Path;

use crate::assets::{FontAsset, Preferences};
use crate::graphics::{Framebuffer, SCREEN_H, SCREEN_W};

pub const VIDEO_MODES: [&str; 6] = [
    "MCGA / VGA 256 Color",
    "VGA 256 Color",
    "EGA 16 Color",
    "Tandy 16 Color",
    "CGA 4 Color (RGB)",
    "CGA Composite",
];

#[derive(Clone, Debug)]
pub struct SetupMenu {
    pub prefs: Preferences,
    pub selected_row: usize,
    pub error: Option<String>,
    revision: u64,
}

impl SetupMenu {
    pub fn new(prefs: Preferences) -> Self {
        Self {
            prefs,
            selected_row: 0,
            error: None,
            revision: 0,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn set_prefs(&mut self, prefs: Preferences) {
        if self.prefs != prefs {
            self.prefs = prefs;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn set_video_mode(&mut self, video_mode: u16) {
        if self.prefs.video_mode != video_mode {
            self.prefs.video_mode = video_mode;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn clear_error(&mut self) {
        if self.error.take().is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn num_rows(&self) -> usize {
        7
    }

    pub fn move_up(&mut self) {
        if self.selected_row > 0 {
            self.selected_row -= 1;
        } else {
            self.selected_row = self.num_rows() - 1;
        }
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn move_down(&mut self) {
        self.selected_row = (self.selected_row + 1) % self.num_rows();
        self.revision = self.revision.wrapping_add(1);
    }

    /// Toggles the currently selected setting; returns `true` if "Save & Return" was activated.
    pub fn activate_or_cycle(&mut self, game_dir: &Path) -> bool {
        let toggle = |v: &mut u16| *v = u16::from(*v == 0);
        match self.selected_row {
            0 => self.prefs.video_mode = if self.prefs.video_mode >= 2 { 1 } else { 2 },
            1 => toggle(&mut self.prefs.music_on),
            2 => toggle(&mut self.prefs.sfx_on),
            3 => toggle(&mut self.prefs.timer_on),
            4 => toggle(&mut self.prefs.grid_lines_on),
            5 => toggle(&mut self.prefs.text_msg_on),
            6 => match self.prefs.save(&game_dir.join("mjp_config.pre")) {
                Ok(()) => {
                    self.clear_error();
                    return true;
                }
                Err(err) => {
                    self.error = Some(err);
                    self.revision = self.revision.wrapping_add(1);
                    return false;
                }
            },
            _ => return false,
        }
        self.revision = self.revision.wrapping_add(1);
        false
    }

    pub fn handle_click(&mut self, mx: i32, my: i32, game_dir: &Path) -> bool {
        if (12..308).contains(&mx) && (32..32 + self.num_rows() as i32 * 14).contains(&my) {
            let row = ((my - 32) / 14) as usize;
            if row < self.num_rows() {
                self.selected_row = row;
                return self.activate_or_cycle(game_dir);
            }
        }
        false
    }

    pub fn render(&self, fb: &mut Framebuffer, font: &FontAsset) {
        fb.reset_clip();
        fb.clear(1); // Classic DOS blue setup background
        fb.draw_rect_outline(4, 4, SCREEN_W - 8, SCREEN_H - 8, 15);
        fb.draw_rect_outline(6, 6, SCREEN_W - 12, SCREEN_H - 12, 11);

        fb.fill_rect(10, 10, SCREEN_W - 20, 16, 4);
        fb.draw_rect_outline(10, 10, SCREEN_W - 20, 16, 15);
        fb.draw_text_shadowed(
            font,
            "MICKEY'S JIGSAW PUZZLES - SYSTEM SETUP",
            48,
            13,
            0,
            14,
        );

        let on_off = |v: u16| if v != 0 { "ON" } else { "OFF" };
        let video = VIDEO_MODES
            .get(self.prefs.video_mode as usize)
            .copied()
            .unwrap_or(VIDEO_MODES[1]);
        let rows: [(&str, &str); 7] = [
            ("Video Mode:", video),
            ("Music:", on_off(self.prefs.music_on)),
            ("Sound Effects:", on_off(self.prefs.sfx_on)),
            ("Puzzle Timer:", on_off(self.prefs.timer_on)),
            ("Grid Lines:", on_off(self.prefs.grid_lines_on)),
            ("Speech Text:", on_off(self.prefs.text_msg_on)),
            ("Save & Return", "ENTER / Click"),
        ];

        for (i, (label, val)) in rows.iter().enumerate() {
            let y = 32 + (i as i32) * 14;
            if i == self.selected_row {
                fb.fill_rect(12, y - 1, SCREEN_W - 24, 13, 3);
            }
            let label_col = if i == self.selected_row { 15 } else { 7 };
            let val_col = if i == self.selected_row { 14 } else { 11 };
            fb.draw_text_shadowed(font, label, 18, y, 0, label_col);
            fb.draw_text_shadowed(font, val, 148, y, 0, val_col);
        }
        if self.error.is_some() {
            fb.draw_text(
                font,
                "Save failed; check directory permissions",
                14,
                145,
                14,
            );
        }
        fb.draw_text(font, "F1 Hint  F2 Solve  F3 Grid  F5 Print", 14, 160, 15);
        fb.draw_text(font, "WASD: pointer   Enter: click", 14, 175, 15);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonzero_preferences_can_be_disabled_and_reenabled() {
        for value in [1, 2, 3, u16::MAX] {
            let mut menu = SetupMenu::new(Preferences {
                music_on: value,
                sfx_on: value,
                timer_on: value,
                grid_lines_on: value,
                text_msg_on: value,
                ..Preferences::default()
            });
            for row in 1..=5 {
                menu.selected_row = row;
                assert!(!menu.activate_or_cycle(Path::new("/dev/null")));
            }
            assert_eq!(
                [
                    menu.prefs.music_on,
                    menu.prefs.sfx_on,
                    menu.prefs.timer_on,
                    menu.prefs.grid_lines_on,
                    menu.prefs.text_msg_on
                ],
                [0; 5]
            );
            for row in 1..=5 {
                menu.selected_row = row;
                assert!(!menu.activate_or_cycle(Path::new("/dev/null")));
            }
            assert_eq!(
                [
                    menu.prefs.music_on,
                    menu.prefs.sfx_on,
                    menu.prefs.timer_on,
                    menu.prefs.grid_lines_on,
                    menu.prefs.text_msg_on
                ],
                [1; 5]
            );
        }
    }
}
